//! Cold, source-ordered ACE Shop construction and one exact valuable operation.
use bace_content::WeenieV1;
use bace_inventory::ItemPlace;
use bace_persistence::{
    DurableItemPlace, PlacementChange, PlacementOperation, SaveSnapshot, VendorStockOperation,
    VendorStockWrite,
};
use bace_simulation::{
    PreparedVendorLazyItem, PreparedVendorLazyStock, PreparedVendorTree, VendorLazyStockReceipt,
    VendorLazyStockTicket,
};
use bace_storage_codec::{
    EntitySaveV1, ItemPlacementV2, ItemSaveV2, ItemSaveV3, ItemSaveV4, ItemSaveV5, PackGeneration,
    PackKey, PackLookup, VendorDefaultStockV1, VendorStockSaveV1,
};
use bace_types::EntityId;
use bace_wire::{ObjectCodecLimits, VendorListing, VendorListingItem};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(super) struct Input {
    pub vendor: EntityId,
    pub vendor_expected_version: i64,
    pub source: WeenieV1,
    pub source_revision: u64,
    pub source_hash: [u8; 32],
    pub world_epoch: u64,
    pub max_message_bytes: usize,
    pub operation_id: String,
    /// Marker first, then every source-ordered root and descendant.
    pub ids: Vec<EntityId>,
    pub templates: BTreeMap<u32, Arc<WeenieV1>>,
}

pub(super) struct Prepared {
    pub batch: PreparedVendorLazyStock,
    pub listing: VendorListing,
    snapshots: Vec<SaveSnapshot>,
    changes: Vec<PlacementChange>,
    defaults: Vec<VendorDefaultStockV1>,
}

pub(super) struct Restored {
    pub batch: PreparedVendorLazyStock,
    pub receipt: VendorLazyStockReceipt,
    pub listing: VendorListing,
}

pub(super) struct RestoreInput {
    pub vendor: EntityId,
    pub vendor_expected_version: i64,
    pub source: WeenieV1,
    pub source_revision: u64,
    pub source_hash: [u8; 32],
    pub max_message_bytes: usize,
    pub forest: bace_persistence::StoredVendorStockForest,
}

/// A later marker version is admitted only for default Buy history: those
/// transactions advance marker/stock revisions together without changing any
/// vendor stock object. Sell, unique, generated contributions and edited stock
/// continue to require their own reconstruction owner.
pub(super) fn restore(input: RestoreInput) -> Result<Restored, String> {
    let marker =
        VendorStockSaveV1::decode(&input.forest.marker.bytes).map_err(|error| error.to_string())?;
    let base_revision = u64::try_from(marker.defaults.len())
        .ok()
        .and_then(|count| count.checked_add(1))
        .ok_or("vendor loaded marker base revision")?;
    let purchases = input
        .forest
        .marker
        .persisted_version
        .checked_sub(1)
        .and_then(|count| u64::try_from(count).ok())
        .ok_or("vendor loaded marker version")?;
    let expected_revision = base_revision
        .checked_add(purchases)
        .ok_or("vendor loaded marker stock revision")?;
    if marker.vendor_object_id != input.vendor.0
        || marker.source_revision != input.source_revision
        || marker.source_hash != input.source_hash
        || marker.marker_object_id != input.forest.marker.marker_object_id
        || !marker.unique.is_empty()
        || marker.stock_revision != expected_revision
        || marker
            .defaults
            .iter()
            .any(|entry| entry.contribution_units != 0)
    {
        return Err(
            "vendor loaded marker requires unsupported commerce or source reconciliation".into(),
        );
    }
    let mut cursor = 0;
    let mut entries = Vec::with_capacity(marker.defaults.len());
    let mut receipt_items = Vec::with_capacity(input.forest.items.len());
    for default in &marker.defaults {
        let count = default.child_ids.len() + 1;
        let rows = input
            .forest
            .items
            .get(cursor..cursor + count)
            .ok_or("vendor loaded forest order")?;
        cursor += count;
        if rows[0].aggregate.object_id != default.root
            || rows
                .iter()
                .skip(1)
                .map(|row| row.aggregate.object_id)
                .collect::<Vec<_>>()
                != default.child_ids
        {
            return Err("vendor loaded tree identities".into());
        }
        let mut root_template = None;
        let mut children = Vec::with_capacity(count - 1);
        let mut containers = Vec::new();
        let mut templates = BTreeMap::new();
        let mut destinations = BTreeMap::new();
        for (index, row) in rows.iter().enumerate() {
            if row.aggregate.persisted_version != 1 {
                return Err("vendor loaded item has later mutation".into());
            }
            let saved =
                ItemSaveV5::decode(&row.aggregate.bytes).map_err(|error| error.to_string())?;
            if saved.entity.mutation_revision != 1
                || saved.entity.template_revision != input.source_revision
            {
                return Err("vendor loaded item source revision".into());
            }
            let ItemPlacementV2::Contained {
                container,
                slot,
                pack_slot,
                equipped: 0,
            } = saved.placement
            else {
                return Err("vendor loaded item containment".into());
            };
            if index == 0 && container != input.vendor.0 {
                return Err("vendor loaded root parent".into());
            }
            let id = EntityId(row.aggregate.object_id);
            let (item, container_state) = crate::generator_items::prepare_inventory_item(
                &saved.entity.state,
                id,
                1,
                ItemPlace::Contained {
                    container: EntityId(container),
                    slot,
                    equipped: 0,
                },
            )?;
            if item.pack_slot != pack_slot {
                return Err("vendor loaded pack slot".into());
            }
            if let Some(container_state) = container_state {
                containers.push(container_state);
            }
            if index == 0 {
                root_template = Some(Arc::new(saved.entity.state.clone()));
            } else {
                children.push(item);
                templates.insert(id, Arc::new(saved.entity.state.clone()));
            }
            destinations.insert(id, saved.source_destination);
            receipt_items.push((id, row.aggregate.persisted_version));
        }
        entries.push(PreparedVendorLazyItem {
            tree: PreparedVendorTree {
                root: EntityId(default.root),
                template: root_template.ok_or("vendor loaded root missing")?,
                items: children,
                containers,
                templates,
            },
            display_quantity: default.display_quantity,
            source_destinations: destinations,
        });
    }
    if cursor != input.forest.items.len() {
        return Err("vendor loaded forest trailing item".into());
    }
    let listing = listing(&input.source, input.vendor, &entries)?;
    listing
        .encode(0, 0, 1024, listing_limits(input.max_message_bytes))
        .map_err(|error| format!("vendor loaded ApproachVendor preflight: {error:?}"))?;
    let batch = PreparedVendorLazyStock {
        vendor: input.vendor,
        vendor_expected_version: input.vendor_expected_version,
        marker: EntityId(marker.marker_object_id),
        operation_id: format!("vendor-restore-{}", marker.marker_object_id),
        source_revision: input.source_revision,
        source_hash: input.source_hash,
        expected_stock_revision: 0,
        entries,
    };
    let receipt = VendorLazyStockReceipt {
        vendor: input.vendor,
        marker: batch.marker,
        operation_id: batch.operation_id.clone(),
        marker_version: input.forest.marker.persisted_version,
        items: receipt_items,
    };
    Ok(Restored {
        batch,
        receipt,
        listing,
    })
}

/// Reopen only templates reachable from this vendor's Shop rows and their
/// contained CreateList descendants. Missing top-level factory templates are
/// omitted by ACE; a missing nested source holds the whole construction.
pub(super) fn load_templates(
    generation: &PackGeneration,
    vendor: &WeenieV1,
) -> Result<BTreeMap<u32, Arc<WeenieV1>>, String> {
    let mut templates = BTreeMap::new();
    let mut queue: Vec<(u32, usize)> = vendor
        .properties
        .create_list
        .iter()
        .filter(|row| row.destination_type == 4)
        .map(|row| (row.weenie_class_id, 0))
        .collect();
    while let Some((id, depth)) = queue.pop() {
        if depth > 32 || queue.len() > 4096 || templates.len() > 4096 {
            return Err("vendor Shop template closure capacity".into());
        }
        if templates.contains_key(&id) {
            continue;
        }
        let PackLookup::Record(record) = generation
            .lookup(PackKey {
                namespace: 1,
                id: u64::from(id),
            })
            .map_err(|error| error.to_string())?
        else {
            if depth == 0 {
                continue;
            }
            return Err("vendor Shop nested template missing".into());
        };
        let source: WeenieV1 =
            bace_content_tools::decode(record.bytes()).map_err(|error| error.to_string())?;
        if source.weenie_id != id {
            return Err("vendor Shop template identity".into());
        }
        for row in source
            .properties
            .create_list
            .iter()
            .filter(|row| row.destination_type == 1)
        {
            queue.push((row.weenie_class_id, depth + 1));
        }
        templates.insert(id, Arc::new(source));
    }
    Ok(templates)
}

pub(super) fn required_ids(
    source: &WeenieV1,
    templates: &BTreeMap<u32, Arc<WeenieV1>>,
    max_message_bytes: usize,
) -> Result<usize, String> {
    let trees = bace_loot::materialize_vendor_shop_create_list(source, templates)
        .map_err(|error| format!("vendor Shop CreateList: {error:?}"))?;
    let preflight: Vec<_> = trees
        .iter()
        .map(|tree| {
            Ok(PreparedVendorLazyItem {
                tree: PreparedVendorTree {
                    root: EntityId(1),
                    template: Arc::new(
                        tree.items
                            .first()
                            .ok_or("vendor Shop empty tree")?
                            .source
                            .clone(),
                    ),
                    items: vec![],
                    containers: vec![],
                    templates: BTreeMap::new(),
                },
                display_quantity: tree.display_quantity,
                source_destinations: BTreeMap::new(),
            })
        })
        .collect::<Result<_, String>>()?;
    listing(source, EntityId(1), &preflight)?
        .encode(0, 0, 1024, listing_limits(max_message_bytes))
        .map_err(|error| format!("vendor Shop listing preflight: {error:?}"))?;
    trees
        .iter()
        .try_fold(1_usize, |total, tree| total.checked_add(tree.items.len()))
        .filter(|total| *total <= 1025)
        .ok_or("vendor Shop identity capacity".into())
}

pub(super) fn prepare(input: Input) -> Result<Prepared, String> {
    if input.vendor.0 == 0
        || input.vendor_expected_version <= 0
        || input.source.weenie_type != 12
        || input
            .source
            .properties
            .data_ids
            .iter()
            .any(|property| property.id == 57)
        || input.source_revision == 0
        || input.source_hash == [0; 32]
        || input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || input.operation_id.is_empty()
        || input.operation_id.len() > 128
        || !input
            .operation_id
            .bytes()
            .all(|byte| byte.is_ascii_graphic())
        || input
            .ids
            .iter()
            .any(|id| id.0 < 0x8000_0000 || *id == input.vendor)
        || input.ids.iter().copied().collect::<BTreeSet<_>>().len() != input.ids.len()
    {
        return Err("vendor Shop source or identity".into());
    }
    // Listing publication is required for Use completion. Hold unsupported
    // alternate currency and incomplete authored pricing before reserving IDs.
    for property in [74, 75, 76] {
        if input
            .source
            .properties
            .ints
            .iter()
            .find(|entry| entry.id == property)
            .is_none_or(|entry| entry.value < 0)
        {
            return Err("vendor listing integer source missing".into());
        }
    }
    for property in [37, 38] {
        if input
            .source
            .properties
            .floats
            .iter()
            .find(|entry| entry.id == property)
            .is_none_or(|entry| !entry.value.is_finite() || entry.value < 0.0)
        {
            return Err("vendor listing price source missing".into());
        }
    }
    let raw = bace_loot::materialize_vendor_shop_create_list(&input.source, &input.templates)
        .map_err(|error| format!("vendor Shop CreateList: {error:?}"))?;
    let needed = raw
        .iter()
        .try_fold(1_usize, |total, tree| total.checked_add(tree.items.len()))
        .filter(|total| *total <= 1025)
        .ok_or("vendor Shop identity capacity")?;
    if input.ids.len() != needed {
        return Err("vendor Shop allocated identity count".into());
    }
    let marker = input.ids[0];
    let mut at = 1;
    let mut entries = Vec::with_capacity(raw.len());
    let mut snapshots = Vec::with_capacity(needed - 1);
    let mut changes = Vec::with_capacity(needed - 1);
    let mut defaults = Vec::with_capacity(raw.len());
    for (root_slot, raw_tree) in raw.into_iter().enumerate() {
        let end = at + raw_tree.items.len();
        let ids = &input.ids[at..end];
        at = end;
        let root = ids[0];
        let mut descendants = Vec::with_capacity(ids.len() - 1);
        let mut containers = Vec::new();
        let mut templates = BTreeMap::new();
        let mut source_destinations = BTreeMap::new();
        let mut root_template = None;
        for (index, (id, prepared)) in ids.iter().copied().zip(&raw_tree.items).enumerate() {
            let parent = match prepared.parent_index {
                Some(parent) if parent < index => ids[parent],
                None if index == 0 => input.vendor,
                _ => return Err("vendor Shop parent order".into()),
            };
            let slot = if index == 0 {
                u32::try_from(root_slot).map_err(|_| "vendor Shop root slot")?
            } else {
                prepared.inventory_slot
            };
            let place = ItemPlace::Contained {
                container: parent,
                slot,
                equipped: 0,
            };
            let (item, container) =
                crate::generator_items::prepare_inventory_item(&prepared.source, id, 1, place)?;
            if index == 0 {
                root_template = Some(Arc::new(prepared.source.clone()));
            } else {
                descendants.push(item.clone());
                templates.insert(id, Arc::new(prepared.source.clone()));
            }
            if let Some(container) = container {
                containers.push(container);
            }
            source_destinations.insert(id, prepared.source_destination);
            let placement = ItemPlacementV2::Contained {
                container: parent.0,
                slot,
                pack_slot: item.pack_slot,
                equipped: 0,
            };
            let saved = ItemSaveV5 {
                previous: ItemSaveV4 {
                    previous: ItemSaveV3 {
                        previous: ItemSaveV2 {
                            entity: EntitySaveV1 {
                                object_id: id.0,
                                template_revision: input.source_revision,
                                mutation_revision: 1,
                                state: prepared.source.clone(),
                            },
                            placement,
                        },
                        enchantments: vec![],
                    },
                    construction: None,
                },
                source_destination: prepared.source_destination,
            };
            snapshots.push(SaveSnapshot {
                object_id: id.0,
                mutation_revision: 1,
                expected_version: 0,
                bytes: saved.encode().map_err(|error| error.to_string())?,
            });
            changes.push(PlacementChange {
                item: id.0,
                expected: None,
                destination: DurableItemPlace::Contained {
                    container: parent.0,
                    slot,
                    pack_slot: item.pack_slot,
                    equipped: 0,
                },
            });
        }
        defaults.push(VendorDefaultStockV1 {
            root: root.0,
            display_quantity: raw_tree.display_quantity,
            contribution_units: 0,
            child_ids: ids[1..].iter().map(|id| id.0).collect(),
        });
        entries.push(PreparedVendorLazyItem {
            tree: PreparedVendorTree {
                root,
                template: root_template.ok_or("vendor Shop empty tree")?,
                items: descendants,
                containers,
                templates,
            },
            display_quantity: raw_tree.display_quantity,
            source_destinations,
        });
    }
    let listing = listing(&input.source, input.vendor, &entries)?;
    listing
        .encode(0, 0, 1024, listing_limits(input.max_message_bytes))
        .map_err(|error| format!("vendor ApproachVendor preflight: {error:?}"))?;
    Ok(Prepared {
        listing,
        batch: PreparedVendorLazyStock {
            vendor: input.vendor,
            vendor_expected_version: input.vendor_expected_version,
            marker,
            operation_id: input.operation_id,
            source_revision: input.source_revision,
            source_hash: input.source_hash,
            expected_stock_revision: 0,
            entries,
        },
        snapshots,
        changes,
        defaults,
    })
}

pub(super) fn listing(
    source: &WeenieV1,
    vendor: EntityId,
    entries: &[PreparedVendorLazyItem],
) -> Result<VendorListing, String> {
    let int = |id| -> Result<u32, String> {
        let value = source
            .properties
            .ints
            .iter()
            .find(|property| property.id == id)
            .ok_or("vendor listing integer missing")?
            .value;
        u32::try_from(value).map_err(|_| "vendor listing integer negative".into())
    };
    let price = |id| -> Result<f32, String> {
        let value = source
            .properties
            .floats
            .iter()
            .find(|property| property.id == id)
            .ok_or("vendor listing price missing")?
            .value;
        let value = value as f32;
        if value.is_finite() && value >= 0.0 {
            Ok(value)
        } else {
            Err("vendor listing price invalid".into())
        }
    };
    if source
        .properties
        .data_ids
        .iter()
        .any(|property| property.id == 57)
    {
        return Err("alternate-currency vendor listing needs player balance".into());
    }
    let mut items = Vec::with_capacity(entries.len());
    for entry in entries {
        items.push(VendorListingItem {
            quantity: entry.display_quantity,
            object_id: entry.tree.root.0,
            game: crate::player_entry::prepare_vendor_game_data(&entry.tree.template)?,
        });
    }
    Ok(VendorListing {
        vendor_id: vendor.0,
        merchandise_types: int(74)?,
        minimum_value: int(75)?,
        maximum_value: int(76)?,
        deal_magical: source
            .properties
            .bools
            .iter()
            .find(|property| property.id == 39)
            .is_some_and(|property| property.value),
        buy_price: price(37)?,
        sell_price: price(38)?,
        alternate_currency: None,
        items,
    })
}

pub(super) fn listing_limits(max_message_bytes: usize) -> ObjectCodecLimits {
    ObjectCodecLimits {
        max_string_bytes: 4096,
        max_model_entries: 4096,
        max_children: 1024,
        max_restrictions: 1024,
        max_motion_commands: 1024,
        max_message_bytes,
    }
}

impl Prepared {
    pub(super) fn freeze(
        self,
        ticket: &VendorLazyStockTicket,
        world_epoch: u64,
    ) -> Result<VendorStockOperation, String> {
        if world_epoch == 0
            || world_epoch > i64::MAX as u64
            || ticket.vendor != self.batch.vendor
            || ticket.vendor_expected_version != self.batch.vendor_expected_version
            || ticket.marker != self.batch.marker
            || ticket.operation_id != self.batch.operation_id
            || ticket.source_revision != self.batch.source_revision
            || ticket.source_hash != self.batch.source_hash
            || ticket.expected_stock_revision != 0
            || ticket.stock_revision
                != u64::try_from(self.defaults.len()).map_err(|_| "vendor Shop stock count")? + 1
            || ticket.item_ids.iter().map(|id| id.0).collect::<Vec<_>>()
                != self
                    .snapshots
                    .iter()
                    .map(|snapshot| snapshot.object_id)
                    .collect::<Vec<_>>()
        {
            return Err("vendor Shop reservation mismatch".into());
        }
        let marker = VendorStockSaveV1 {
            marker_object_id: ticket.marker.0,
            vendor_object_id: ticket.vendor.0,
            source_revision: ticket.source_revision,
            source_hash: ticket.source_hash,
            loaded: true,
            stock_revision: ticket.stock_revision,
            defaults: self.defaults,
            unique: vec![],
        };
        let mut participants = Vec::with_capacity(self.snapshots.len() + 2);
        participants.extend([ticket.vendor.0, ticket.marker.0]);
        participants.extend(ticket.item_ids.iter().map(|id| id.0));
        Ok(VendorStockOperation {
            world_epoch,
            vendor_expected_version: ticket.vendor_expected_version,
            inventory: PlacementOperation {
                operation_id: ticket.operation_id.clone(),
                snapshots: self.snapshots,
                participants,
                leases: vec![],
                changes: self.changes,
                storage_views: vec![],
            },
            marker: VendorStockWrite {
                marker_object_id: ticket.marker.0,
                expected_version: 0,
                expected_stock_revision: 0,
                mutation_revision: 1,
                bytes: marker.encode().map_err(|error| error.to_string())?,
            },
        })
    }
}
