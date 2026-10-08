//! Equipment cold work is bounded to the one retained inventory request. All
//! effects and placement are revalidated by simulation before saving.
use super::*;
use bace_inventory::ItemPlace;
use bace_simulation::{
    PlayerReadSnapshot, PlayerSnapshotOutcome, PlayerSnapshotRequest, PreparedEquipmentPhysical,
};
use bace_storage_codec::{ItemPlacementV2, ItemSaveV2};
pub(super) struct EquipmentRuntime {
    pub(super) visibility: Option<crate::inventory_equipment_output::EquipmentVisibilityUpdate>,
    pub(super) visibility_accepted: bool,
    pub(super) fresh_registered: bool,
    pub(super) failure: Option<String>,
    context: ActionContext,
    request: InventoryRequest,
    capture: Option<u64>,
    snapshot: Option<(Arc<PlayerReadSnapshot>, u64)>,
    started: bool,
    physical: Option<Job<Result<PreparedEquipmentPhysical, String>>>,
    ready: Option<PreparedEquipmentPhysical>,
}
impl GameRuntime {
    pub(super) fn begin_inventory_equipment(
        &mut self,
        key: SessionKey,
        binding: CharacterBinding,
        context: ActionContext,
        request: InventoryRequest,
    ) -> Result<(), String> {
        let correlation = self.token()?;
        self.inventory.equipment = Some(EquipmentRuntime {
            visibility: None,
            visibility_accepted: false,
            fresh_registered: false,
            failure: None,
            context,
            request,
            capture: None,
            snapshot: None,
            started: false,
            physical: None,
            ready: None,
        });
        self.inventory.pending = Some(Pending {
            key,
            item: item_id(request),
            rejected: false,
            binding,
            correlation,
            request: None,
            prepared: None,
            saving: false,
            sources_registered: false,
            inspection: None,
            motions: VecDeque::new(),
            completion: None,
        });
        self.inventory.failures.remove(&key);
        Ok(())
    }
    pub(in crate::game_runtime) fn inventory_owns_capture(
        &self,
        outcome: &PlayerSnapshotOutcome,
    ) -> bool {
        self.inventory.service.owns_capture(outcome)
            || self
                .inventory
                .equipment
                .as_ref()
                .is_some_and(|p| p.capture == Some(outcome.correlation))
    }
    pub(in crate::game_runtime) fn accept_inventory_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if self.inventory.service.owns_capture(&outcome) {
            return self.inventory.service.accept_capture(outcome, unix);
        }
        let Some(p) = self
            .inventory
            .equipment
            .as_mut()
            .filter(|p| p.capture == Some(outcome.correlation))
        else {
            return Err(outcome);
        };
        p.capture = None;
        let owner = self
            .inventory
            .pending
            .as_mut()
            .expect("equipment request owner");
        match outcome.result {
            Ok(snapshot)
                if snapshot.binding() == owner.binding && snapshot.operation().is_none() =>
            {
                p.snapshot = Some((snapshot, unix))
            }
            result => {
                owner.rejected = true;
                self.inventory
                    .failures
                    .insert(owner.key, format!("equipment capture rejected: {result:?}"));
            }
        }
        Ok(())
    }
    pub(super) fn poll_inventory_equipment(&mut self) -> Result<(), String> {
        if self.inventory.pending.is_none() {
            self.inventory.equipment = None;
            return Ok(());
        }
        let Some(gear) = self.inventory.equipment.as_ref() else {
            return Ok(());
        };
        if self.inventory.pending.as_ref().is_some_and(|p| p.rejected) {
            return Ok(());
        }
        if !gear.started && gear.capture.is_none() && gear.snapshot.is_none() {
            let correlation = self.token()?;
            let binding = self
                .inventory
                .pending
                .as_ref()
                .expect("equipment owner")
                .binding;
            match self.simulation.input().try_submit(Command::PlayerSnapshot(
                PlayerSnapshotRequest {
                    correlation,
                    binding,
                    operation: None,
                },
            )) {
                Ok(()) => {
                    self.inventory
                        .equipment
                        .as_mut()
                        .expect("equipment owner")
                        .capture = Some(correlation)
                }
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    return Err("equipment capture ingress closed".into());
                }
            }
        }
        if self
            .inventory
            .equipment
            .as_ref()
            .is_some_and(|p| !p.started && p.snapshot.is_some())
            && self.inventory.cold.is_none()
        {
            let gear = self.inventory.equipment.as_ref().expect("equipment owner");
            let (snapshot, unix) = gear.snapshot.as_ref().expect("equipment capture");
            let baseline = self
                .online_saves
                .baseline(snapshot.binding().actor.0)
                .ok_or("equipment baseline missing")?
                .0;
            let player = crate::player_saves::freeze_player_snapshot(baseline, snapshot, *unix)
                .map_err(|e| e.to_string())?;
            let sources = self.online_saves.captured_inventory_baselines(snapshot)?;
            let context = gear.context;
            let request = gear.request;
            let snapshot = snapshot.clone();
            let manifest = self.bootstrap.assets.clone();
            let rows = self.assets.spell_rows.clone();
            let store = self.bootstrap.store.clone();
            let generation = self.bootstrap.pack.generation.clone();
            self.inventory.cold = Some(Box::pin(async move {
                let fresh_id = if matches!(request, InventoryRequest::SplitToWield { .. }) {
                    let ids = store
                        .allocate_dynamic_ids(1)
                        .await
                        .map_err(|e| e.to_string())?;
                    Some(EntityId(
                        *ids.first()
                            .filter(|_| ids.len() == 1)
                            .ok_or("split-to-wield identity count")?,
                    ))
                } else {
                    None
                };
                tokio::task::spawn_blocking(move || {
                    let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
                    let dat = Arc::new(assets.prepare_avatar_dat(&player.player.entity.state)?);
                    let mut items: Vec<_> = sources.into_iter()
                        .filter(|i| i.entity.object_id == item_id(request).0
                            || matches!(i.placement, Some(ItemPlacementV2::Contained { equipped, .. }) if equipped != 0))
                        .map(|i| Ok(bace_storage_codec::ItemSaveV4 {
                            previous: bace_storage_codec::ItemSaveV3 {
                                previous: ItemSaveV2 {
                                    entity: i.entity,
                                    placement: i.placement.ok_or("equipment item placement missing")?,
                                },
                                enchantments: i.enchantments,
                            },
                            construction: i.construction,
                        }))
                        .collect::<Result<_, String>>()?;
                    let before_location = items.iter()
                        .find(|i| i.entity.object_id == item_id(request).0)
                        .and_then(|i| match i.placement {
                            ItemPlacementV2::Contained { equipped, .. } => Some(equipped),
                            _ => None,
                        })
                        .ok_or("equipment source placement missing")?;
                    let pretransition = snapshot.combat_mode().is_some_and(|mode| mode != 1)
                        && matches!(request, InventoryRequest::Move { .. })
                        && matches!(before_location, 0x100000 | 0x200000 | 0x400000 | 0x1000000 | 0x2000000);
                    let stance = if pretransition {
                        let current = snapshot.entry_motion().ok_or("equipment accepted motion missing")?;
                        if current.style == 0x8000_003c { None } else {
                            let scale = player.player.entity.state.properties.floats.iter()
                                .find(|p| p.id == 39).map_or(1., |p| p.value) as f32;
                            Some(crate::world_admission::prepare_motion_chain(
                                &dat.motions, &dat.animations,
                                crate::world_admission::MotionChainRequest {
                                    style: current.style,
                                    current_motion: current.substate,
                                    current_speed: current.speed,
                                    action: 0x8000_003c,
                                    action_speed: 1.,
                                    scale,
                                    modifiers: &[],
                                },
                            )?)
                        }
                    } else { None };
                    let mut authority = cold::authority(context.actor);
                    let mut split = None;
                    let mut fresh = None;
                    let target_id = if let Some(id) = fresh_id {
                        let source = items.iter().find(|i| i.entity.object_id == item_id(request).0)
                            .ok_or("split-to-wield source is not owned")?;
                        let bace_storage_codec::PackLookup::Record(record) = generation.lookup(
                            bace_storage_codec::PackKey { namespace: 1, id: u64::from(source.entity.state.weenie_id) }
                        ).map_err(|e| e.to_string())? else { return Err("split-to-wield template missing".into()); };
                        let template = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
                        authority.new_item = Some(id);
                        let (prepared, stack) = crate::inventory_service::prepare_split_request(
                            crate::inventory_service::SplitPreparationInput {
                                context, request, authority, template: &template,
                                template_revision: generation.revision(), fresh_id: id,
                                source: &source.entity.state, source_vendor: false,
                                destination_corpse: false, wield_requirements_met: true, drop: None,
                            }
                        )?;
                        split = prepared.split;
                        let bace_inventory::ItemPlace::Contained { container, slot, equipped } = stack.item.place
                            else { return Err("split-to-wield final placement missing".into()); };
                        items.push(bace_storage_codec::ItemSaveV4 {
                            previous: bace_storage_codec::ItemSaveV3 {
                                previous: ItemSaveV2 {
                                    entity: stack.frozen.entity.clone(),
                                    placement: ItemPlacementV2::Contained {
                                        container: container.0, slot, pack_slot: false, equipped,
                                    },
                                },
                                enchantments: vec![],
                            },
                            construction: None,
                        });
                        fresh = Some(stack);
                        id
                    } else { item_id(request) };
                    let target = items.iter().find(|i| i.entity.object_id == target_id.0)
                        .ok_or("equipment target is not owned")?;
                    let wield = crate::player_assets::prepare_wield_policy(context.actor, &target.entity.state, 0);
                    let effects = crate::equipment_effects::prepare_equipment_effect_inputs(
                        context.actor, snapshot.character().progression().revision(), &items,
                        snapshot.item_experience(), &dat.spells, &rows,
                    )?;
                    let appearance = assets.prepare_entry_appearance(
                        &std::iter::once(&player.player.entity.state)
                            .chain(items.iter().map(|i| &i.entity.state)).collect::<Vec<_>>()
                    )?;
                    Ok(cold::Prepared {
                        request: Some(InventoryCommandKind::ProposeEquipment(Box::new(
                            bace_simulation::PreparedEquipmentRequest {
                                request: InventoryPreparedRequest {
                                    context, request, authority, split, drop: None,
                                },
                                wield, effects, stance,
                                slots: items.iter().map(crate::player_assets::prepare_wield_slot).collect(),
                            },
                        ))),
                        external: vec![], fresh, appearance: Some(appearance), equipment_dat: Some(dat),
                    })
                }).await.map_err(|e| format!("equipment cold preparation: {e}"))?
            }));
            self.inventory
                .equipment
                .as_mut()
                .expect("equipment owner")
                .started = true;
        }
        if let Some(done) = ready(
            &mut self
                .inventory
                .equipment
                .as_mut()
                .expect("equipment owner")
                .physical,
        ) {
            match done {
                Ok(prepared) => {
                    self.inventory
                        .equipment
                        .as_mut()
                        .expect("equipment owner")
                        .ready = Some(prepared)
                }
                Err(error) => {
                    let key = self
                        .inventory
                        .pending
                        .as_ref()
                        .expect("equipment owner")
                        .key;
                    self.inventory.failures.insert(key, error.clone());
                    self.inventory
                        .equipment
                        .as_mut()
                        .expect("equipment owner")
                        .failure = Some(error);
                    return Ok(());
                }
            }
        }
        if self
            .inventory
            .equipment
            .as_ref()
            .is_some_and(|p| p.ready.is_some())
        {
            let correlation = self.token()?;
            let prepared = self
                .inventory
                .equipment
                .as_mut()
                .expect("equipment owner")
                .ready
                .take()
                .expect("ready equipment");
            if let Err(prepared) = self
                .inventory
                .service
                .supply_equipment_physical(prepared, correlation)
            {
                self.inventory
                    .equipment
                    .as_mut()
                    .expect("equipment owner")
                    .ready = Some(*prepared);
            }
        }
        self.start_captured_equipment_preparation()?;
        Ok(())
    }
}
impl GameRuntime {
    fn start_captured_equipment_preparation(&mut self) -> Result<(), String> {
        let gear = self.inventory.equipment.as_ref().expect("equipment owner");
        if gear.physical.is_some() || gear.ready.is_some() || gear.failure.is_some() {
            return Ok(());
        }
        let Some((work, snapshot, unix)) = self.inventory.service.equipment_capture() else {
            return Ok(());
        };
        let dat = self
            .inventory
            .pending
            .as_ref()
            .and_then(|p| p.prepared.as_ref())
            .and_then(|p| p.equipment_dat.clone())
            .ok_or("equipment DAT closure missing")?;
        let baseline = self
            .online_saves
            .baseline(work.binding.actor.0)
            .ok_or("equipment baseline missing")?
            .0;
        let player = crate::player_saves::freeze_player_operation_baseline(
            baseline,
            snapshot,
            bace_simulation::PlayerSnapshotOperation::Inventory(work.operation.ticket.operation),
            work.operation.actor_revision,
            unix,
        )
        .map_err(|e| e.to_string())?;
        let mut items = self.online_saves.operation_inventory_baselines(snapshot)?;
        if let Some(fresh) = work.fresh.as_ref() {
            if items
                .iter()
                .any(|item| item.entity.object_id == fresh.item.id.0)
            {
                return Err("fresh equipment split already has a baseline".into());
            }
            items.push(fresh.frozen.clone());
        }
        let patch = work
            .operation
            .equipment
            .clone()
            .ok_or("equipment effect companion missing")?;
        crate::equipment_effects::overlay_equipment_item_sources(&mut items, &patch)
            .map_err(|e| e.to_string())?;
        for change in &work.operation.ticket.proposal.changes {
            let source = items
                .iter_mut()
                .find(|i| i.entity.object_id == change.after.id.0)
                .ok_or("equipment proposal item missing")?;
            let ItemPlace::Contained {
                container,
                slot,
                equipped,
            } = change.after.place
            else {
                return Err("equipment candidate leaves carried graph".into());
            };
            source.entity.mutation_revision = change.after.revision;
            source.placement = Some(ItemPlacementV2::Contained {
                container: container.0,
                slot,
                pack_slot: change.after.pack_slot,
                equipped,
            });
            if change.after.maximum_stack > 1
                || source
                    .entity
                    .state
                    .properties
                    .ints
                    .iter()
                    .any(|p| p.id == 12)
            {
                crate::game_inventory::set(
                    &mut source.entity.state.properties.ints,
                    12,
                    i32::try_from(change.after.stack)
                        .map_err(|_| "equipment candidate stack overflow")?,
                );
            }
            if change
                .before
                .as_ref()
                .is_none_or(|before| before.stack != change.after.stack)
                && !change.after.is_container
            {
                crate::game_inventory::set(
                    &mut source.entity.state.properties.ints,
                    5,
                    i32::try_from(
                        u64::from(change.after.stack) * u64::from(change.after.unit_burden),
                    )
                    .map_err(|_| "equipment candidate burden overflow")?,
                );
                crate::game_inventory::set(
                    &mut source.entity.state.properties.ints,
                    19,
                    i32::try_from(
                        u64::from(change.after.stack) * u64::from(change.after.unit_value),
                    )
                    .map_err(|_| "equipment candidate value overflow")?,
                );
            }
        }
        let items = items
            .into_iter()
            .map(|i| {
                Ok(bace_storage_codec::ItemSaveV4 {
                    previous: bace_storage_codec::ItemSaveV3 {
                        previous: ItemSaveV2 {
                            entity: i.entity,
                            placement: i
                                .placement
                                .ok_or("equipment candidate placement missing")?,
                        },
                        enchantments: i.enchantments,
                    },
                    construction: i.construction.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let actor = work.binding.actor;
        let revision = work.operation.actor_revision;
        let fresh_item = work.fresh.as_ref().map(|stack| stack.item.id);
        let snapshot = snapshot.clone();
        let shapes = self.assets.projectile_shapes.clone();
        let manifest = self.bootstrap.assets.clone();
        let generation = self.bootstrap.pack.generation.clone();
        self.inventory
            .equipment
            .as_mut()
            .expect("equipment owner")
            .physical = Some(Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let registry = patch
                    .registry(actor)
                    .map_err(|_| "equipment candidate registry")?;
                let registry = registry
                    .as_ref()
                    .or_else(|| snapshot.enchantments())
                    .ok_or("equipment wearer registry missing")?;
                let mut item_registries = snapshot
                    .item_enchantments()
                    .iter()
                    .map(|(id, r)| {
                        bace_magic::EnchantmentRegistry::restore(
                            r.capacity(),
                            r.revision(),
                            r.entries().to_vec(),
                        )
                        .map(|r| (*id, r))
                        .map_err(|_| "equipment registry snapshot")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if let Some(id) = fresh_item {
                    if item_registries.iter().any(|(item, _)| *item == id) {
                        return Err("fresh equipment registry already exists".into());
                    }
                    item_registries.push((
                        id,
                        bace_magic::EnchantmentRegistry::new(4096)
                            .map_err(|_| "fresh equipment registry capacity")?,
                    ));
                }
                for change in patch.registries.iter().filter(|r| r.actor != actor) {
                    let current = item_registries
                        .iter_mut()
                        .find(|(id, _)| *id == change.actor)
                        .ok_or("equipment registry owner missing")?;
                    if current.1.entries() != change.before {
                        return Err("equipment registry before-image mismatch".into());
                    }
                    current.1 = patch
                        .registry(change.actor)
                        .map_err(|_| "equipment candidate item registry")?
                        .ok_or("equipment candidate item registry missing")?;
                }
                let mut experience = snapshot.item_experience().to_vec();
                for change in &patch.item_experience {
                    let current = experience.iter().position(|p| p.item == change.item);
                    if current.map(|i| &experience[i]) != change.before.as_ref() {
                        return Err("equipment experience before-image mismatch".into());
                    }
                    if let Some(index) = current {
                        experience.remove(index);
                    }
                    if let Some(next) = &change.after {
                        experience.push(next.clone());
                    }
                }
                let mut shapes = (*shapes).clone();
                for item in &items {
                    if matches!(
                        item.placement,
                        ItemPlacementV2::Contained {
                            equipped: 0x800000 | 0x400000,
                            ..
                        }
                    ) && !shapes.contains_key(&item.entity.state.weenie_id)
                    {
                        let mut assets =
                            crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
                        shapes.insert(
                            item.entity.state.weenie_id,
                            assets.prepare_world_item_shape(&item.entity.state)?,
                        );
                    }
                }
                let mut prepared = crate::player_assets::prepare_equipment_physical_view(
                    crate::player_assets::EquipmentPhysicalViewInput {
                        previous_style: snapshot.entry_motion().map(|m| m.style),
                        actor,
                        before_revision: revision,
                        source: &player.player.entity.state,
                        character: snapshot.character().progression(),
                        registry,
                        item_registries: item_registries.iter().map(|(id, r)| (*id, r)).collect(),
                        item_experience: &experience,
                        items: &items,
                        dat: &dat,
                        projectile_shapes: &shapes,
                    },
                )?;
                let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
                prepared.server_magic = assets
                    .prepare_proc_magic(
                        &generation,
                        &dat.spells,
                        &prepared.magic_damage,
                        &prepared.source.profile,
                    )?
                    .batch;
                Ok(prepared)
            })
            .await
            .map_err(|e| format!("equipment physical cold preparation: {e}"))?
        }));
        Ok(())
    }
}
