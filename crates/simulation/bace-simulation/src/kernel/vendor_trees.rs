//! ACE Spawn_Shop retains a constructed Container.Inventory. A merged incoming
//! root discards its entire tree; only the retained stock root counts as a member.
use super::*;
use crate::{
    GeneratorItemAdmission, GeneratorServiceError as G, PreparedVendorTree, VendorContents,
};
use bace_economy::{VendorStockItem, VendorStockReceipt};
use bace_gameplay_api::{
    GeneratorDestination, GeneratorSpawnKey, GeneratorSpawnMember, GeneratorSpawnReceipt,
    GeneratorSpawnResult,
};
use bace_inventory::ItemPlace;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
impl Kernel {
    pub fn generated_vendor_contents(
        &self,
        vendor: EntityId,
        root: EntityId,
    ) -> Option<&VendorContents> {
        self.generated_vendors.get(&vendor)?.contents.get(&root.0)
    }
    pub fn admit_generated_vendor_trees(
        &mut self,
        key: GeneratorSpawnKey,
        trees: &[PreparedVendorTree],
    ) -> Result<GeneratorItemAdmission, G> {
        let request = self.generators.requests.get(&key).ok_or(G::Stale)?;
        if !self.generators.submitted.contains(&key) || trees.is_empty() || trees.len() > 1024 {
            return Err(G::Invalid);
        }
        let GeneratorDestination::Shop { vendor } = request.intent.destination else {
            return Err(G::Invalid);
        };
        let mut ids = BTreeSet::new();
        for tree in trees {
            validate_tree(tree)?;
            for id in std::iter::once(tree.root).chain(tree.items.iter().map(|i| i.id)) {
                if !ids.insert(id) {
                    return Err(G::Invalid);
                }
            }
        }
        if ids.len() != request.entities.len()
            || request.entities.iter().any(|id| !ids.contains(id))
        {
            return Err(G::Invalid);
        }
        let current = self.generated_vendors.get(&vendor).ok_or(G::Missing)?;
        if current.pending_lazy.is_some() || current.pending_buy.is_some() || current.lazy_loaded {
            return Err(G::Busy);
        }
        let mut stock = current.stock.clone();
        let mut templates = current.items.clone();
        let mut contents = current.contents.clone();
        let mut display_quantities = current.display_quantities.clone();
        let lazy_loaded = current.lazy_loaded;
        let committed_lazy = current.committed_lazy.clone();
        let mut members = BTreeMap::<EntityId, u32>::new();
        let mut accepted = Vec::new();
        let mut roots = Vec::with_capacity(trees.len());
        for tree in trees {
            let int = |id| {
                tree.template
                    .properties
                    .ints
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.value)
            };
            let incoming = VendorStockItem {
                id: tree.root.0,
                template: tree.template.weenie_id,
                stack: int(12),
                maximum_stack: int(11),
            };
            let proposal = stock.propose_default(incoming).map_err(|_| G::Invalid)?;
            let receipt = stock.commit_default(proposal).map_err(|_| G::Invalid)?;
            let contribution = match receipt {
                VendorStockReceipt::Added { id } => {
                    if templates
                        .len()
                        .checked_add(tree.items.len() + 1)
                        .is_none_or(|n| n > 4096)
                    {
                        return Err(G::Capacity);
                    }
                    templates.insert(id, tree.template.clone());
                    display_quantities.insert(id, incoming.stack.unwrap_or(1));
                    accepted.push(tree.root);
                    for (&id, template) in &tree.templates {
                        templates.insert(id.0, template.clone());
                        accepted.push(id);
                    }
                    contents.insert(
                        id,
                        VendorContents {
                            items: tree.items.clone(),
                            containers: tree.containers.clone(),
                        },
                    );
                    u32::try_from(incoming.stack.unwrap_or(1)).map_err(|_| G::Invalid)?
                }
                VendorStockReceipt::Stacked {
                    retained_id, stack, ..
                } => {
                    let mut template = (**templates.get(&retained_id).ok_or(G::Invalid)?).clone();
                    bace_loot::set_treasure_stack(&mut template, stack).map_err(|_| G::Invalid)?;
                    templates.insert(retained_id, Arc::new(template));
                    display_quantities.entry(retained_id).or_insert(stack);
                    1
                }
            };
            let retained = EntityId(receipt.retained_id());
            roots.push(retained);
            let units = members.entry(retained).or_default();
            *units = units.checked_add(contribution).ok_or(G::Invalid)?;
        }
        self.confirm_generator_spawn(GeneratorSpawnReceipt {
            key,
            result: GeneratorSpawnResult::Completed {
                members: members
                    .into_iter()
                    .map(|(entity, contribution)| GeneratorSpawnMember {
                        entity,
                        contribution,
                    })
                    .collect(),
                materialized: true,
                failed_placements: 0,
            },
        })?;
        self.generated_vendors.insert(
            vendor,
            super::vendor_generators::GeneratedVendor {
                stock,
                items: templates,
                contents,
                display_quantities,
                lazy_loaded,
                pending_lazy: None,
                committed_lazy,
                pending_buy: None,
                marker_version: 0,
                marker_stock_revision: 0,
            },
        );
        Ok(GeneratorItemAdmission {
            key,
            roots,
            entities: accepted,
            failed_roots: vec![],
        })
    }
}
pub(in crate::kernel) fn validate_tree(tree: &PreparedVendorTree) -> Result<(), G> {
    if !tree.valid_bounds() || tree.template.weenie_id == 0 {
        return Err(G::Invalid);
    }
    let ids: BTreeSet<_> = tree.items.iter().map(|i| i.id).collect();
    if ids.len() != tree.items.len()
        || ids.contains(&tree.root)
        || !ids.iter().all(|id| tree.templates.contains_key(id))
    {
        return Err(G::Invalid);
    }
    if tree.items.is_empty() && tree.containers.is_empty() {
        return Ok(());
    }
    if tree.containers.iter().filter(|c| c.id == tree.root).count() != 1
        || tree.containers.iter().any(|c| c.root_owner.is_some())
    {
        return Err(G::Invalid);
    }
    let parents: BTreeMap<_, _> = tree.items.iter().map(|i| (i.id, i)).collect();
    for item in &tree.items {
        if item.revision != 1 || item.template != tree.templates[&item.id].weenie_id {
            return Err(G::Invalid);
        }
        let mut cursor = item;
        let mut seen = BTreeSet::from([item.id]);
        loop {
            let ItemPlace::Contained {
                container,
                equipped: 0,
                ..
            } = cursor.place
            else {
                return Err(G::Invalid);
            };
            if container == tree.root {
                break;
            }
            if seen.len() >= 64 || !seen.insert(container) {
                return Err(G::Invalid);
            }
            cursor = parents.get(&container).ok_or(G::Invalid)?;
        }
    }
    // Reuse inventory's slot/capacity and complete-parent checks without touching
    // live inventory. Vendor stock is a separate single-owner economy collection.
    let mut check = crate::inventory::Inventory::new(1024);
    let root = tree
        .containers
        .iter()
        .find(|c| c.id == tree.root)
        .ok_or(G::Invalid)?;
    check.register_container(*root).map_err(|_| G::Invalid)?;
    let descendants: Vec<_> = tree
        .containers
        .iter()
        .filter(|c| c.id != tree.root)
        .copied()
        .collect();
    if !tree.items.is_empty() {
        check
            .register_generated(&tree.items, &descendants)
            .map_err(|_| G::Invalid)?;
    } else if !descendants.is_empty() {
        return Err(G::Invalid);
    }
    Ok(())
}
