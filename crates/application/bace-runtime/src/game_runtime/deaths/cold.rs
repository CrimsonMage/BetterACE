use super::*;
use crate::{
    game_inventory::FrozenInventoryItem,
    player_death_preparation::{
        NoCorpseLootInput, PlayerDeathPreparationInput, PlayerNoCorpsePreparationInput,
    },
};
pub(super) struct Metadata {
    pub fresh_items: Vec<FrozenInventoryItem>,
    pub positions: BTreeMap<u32, bace_content::Position>,
    pub appearance: crate::player_entry::PreparedEntryAppearanceAssets,
    pub corpse_visibility: Option<crate::visibility_assets::PreparedVisibilityObject>,
    pub world_visibility: Vec<crate::visibility_assets::PreparedVisibilityObject>,
}
pub(super) struct Prepared {
    pub command: PlayerDeathCommand,
    pub metadata: Metadata,
}
pub(super) struct Completion {
    pub identities: Vec<EntityId>,
    pub result: Result<Prepared, String>,
}
impl GameRuntime {
    pub(super) fn advance_death_preparation(&mut self, unix: u64) -> Result<(), String> {
        let Some(mut pending) = self.deaths.pending.take() else {
            return Ok(());
        };
        let result = self.advance_death_inner(&mut pending, unix);
        self.deaths.pending = Some(pending);
        if let Err(error) = &result {
            self.deaths.blocked = Some(error.clone());
        }
        result
    }
    fn advance_death_inner(&mut self, p: &mut Pending, _unix: u64) -> Result<(), String> {
        match &mut p.phase {
            Phase::Barrier => {
                if self.online_saves.critical_ready(&[p.binding.actor.0])? {
                    self.online_saves.begin_critical(&[p.binding.actor.0])?;
                    p.phase = Phase::Capture;
                }
            }
            Phase::Capture => {
                let correlation = self.token()?;
                let command = Command::PlayerSnapshot(PlayerSnapshotRequest {
                    correlation,
                    binding: p.binding,
                    operation: Some((
                        PlayerSnapshotOperation::PlayerDeath(p.request.operation),
                        p.request.before,
                    )),
                });
                match self.simulation.input().try_submit(command) {
                    Ok(()) => p.phase = Phase::Capturing(correlation),
                    Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => {
                        return Err("death capture owner closed".into());
                    }
                }
            }
            Phase::Captured(snapshot, unix) => {
                if !snapshot.entry_physics().grounded()
                    || snapshot.entry_physics().velocity() != bace_geometry::Vec3::ZERO
                {
                    p.phase = Phase::Capture;
                    return Ok(());
                }
                let baseline = self
                    .online_saves
                    .baseline(p.binding.actor.0)
                    .ok_or("death baseline missing")?
                    .0;
                let saved = crate::player_saves::freeze_player_operation_baseline(
                    baseline,
                    snapshot,
                    PlayerSnapshotOperation::PlayerDeath(p.request.operation),
                    p.request.before,
                    *unix,
                )
                .map_err(|e| e.to_string())?;
                let items = self.online_saves.operation_inventory_baselines(snapshot)?;
                let world = snapshot.world();
                let half = world.heading * 0.5;
                let location = bace_interactions::PortalPosition {
                    cell: world.cell.0,
                    origin: [world.position.x, world.position.y, world.position.z],
                    rotation: [half.cos(), 0., 0., half.sin()],
                };
                let instantiation = saved
                    .player
                    .entity
                    .state
                    .properties
                    .positions
                    .iter()
                    .find(|p| p.id == 3)
                    .map(|p| bace_interactions::PortalPosition {
                        cell: p.value.obj_cell_id,
                        origin: [p.value.position_x, p.value.position_y, p.value.position_z],
                        rotation: [
                            p.value.rotation_w,
                            p.value.rotation_x,
                            p.value.rotation_y,
                            p.value.rotation_z,
                        ],
                    });
                let destination = bace_interactions::death_destination(
                    snapshot.portal_links().and_then(|links| links.position(4)),
                    instantiation,
                    location,
                )
                .map_err(|e| format!("death destination: {e:?}"))?;
                let blocks = std::collections::BTreeSet::from([
                    (world.cell.0 >> 16) as u16,
                    (destination.cell >> 16) as u16,
                ]);
                for block in blocks {
                    if self
                        .world
                        .as_ref()
                        .is_some_and(|w| w.regions.prepared_region(block).is_some())
                    {
                        continue;
                    }
                    if self.world.is_none() {
                        return Ok(());
                    }
                    if p.regions.insert(block) {
                        let correlation = self.token()?;
                        let command = Command::Generator(bace_simulation::GeneratorCommand {
                            correlation,
                            action: bace_simulation::GeneratorAction::RequestRegion {
                                landblock: block,
                                permanent: false,
                            },
                        });
                        match self.simulation.input().try_submit(command) {
                            Ok(()) => {
                                self.request_regions.insert(correlation, (p.key, block));
                            }
                            Err(TrySendError::Full(_)) => {
                                p.regions.remove(&block);
                            }
                            Err(TrySendError::Disconnected(_)) => {
                                p.regions.remove(&block);
                                return Err("death region owner closed".into());
                            }
                        }
                    }
                    return Ok(());
                }
                let geometry = self
                    .world
                    .as_ref()
                    .and_then(|w| w.regions.prepared_region((world.cell.0 >> 16) as u16))
                    .map(|r| r.geometry.clone())
                    .ok_or("death geometry owner unavailable")?;
                let generation = self.bootstrap.pack.generation.clone();
                let manifest = self.bootstrap.assets.clone();
                let store = self.bootstrap.store.clone();
                let snapshot = snapshot.clone();
                let operation = p.request.operation;
                let killer = p.request.killer.clone();
                let old_ids = p.ids.clone();
                let olthoi_kind = p.request.olthoi;
                let killer_is_olthoi = p.request.killer_is_olthoi;
                let treasure = self
                    .world
                    .as_ref()
                    .ok_or("death world source unavailable")?
                    .regions
                    .treasure_assets();
                let aetheria_rate = self
                    .world
                    .as_ref()
                    .expect("checked world")
                    .regions
                    .aetheria_drop_rate();
                let random = self.bootstrap.random.root();
                let epoch = self.bootstrap.world_owner.epoch();
                let drop_plain_wield = self
                    .bootstrap
                    .config
                    .world
                    .death
                    .creatures_drop_createlist_wield;
                let no_corpse = saved
                    .player
                    .entity
                    .state
                    .properties
                    .bools
                    .iter()
                    .any(|p| p.id == 29 && p.value);
                let unix_seconds = match p.loot_clock {
                    Some(v) => v,
                    None => {
                        let now = i32::try_from(*unix / 1000)
                            .map_err(|_| "death source Unix overflow")?;
                        p.loot_clock = Some(now);
                        now
                    }
                };

                p.proof = Some(snapshot.clone());
                self.deaths.job = Some(Box::pin(async move {
                    let mut identities = old_ids;
                    let count_items = items.clone();
                    let count_generation = generation.clone();
                    let loot_saved = saved.clone();
                    let loot_snapshot = snapshot.clone();
                    let (count, olthoi, no_corpse_loot) =
                        match tokio::task::spawn_blocking(move || {
                            if no_corpse {
                                let loot = crate::player_death_preparation::prepare_no_corpse_loot(
                                    NoCorpseLootInput {
                                        player: &loot_saved,
                                        items: &count_items,
                                        generation: &count_generation,
                                        assets: treasure,
                                        aetheria_rate,
                                        root: random,
                                        epoch,
                                        operation,
                                        killer_is_olthoi,
                                        drop_plain_wield,
                                    },
                                )?;
                                return Ok::<_, String>((loot.sources.len(), None, Some(loot)));
                            }
                            let olthoi = olthoi_kind
                                .map(|kind| {
                                    crate::player_death_preparation::prepare_olthoi_loot(
                                        crate::player_death_preparation::OlthoiLootInput {
                                            kind,
                                            player: &loot_saved,
                                            snapshot: &loot_snapshot,
                                            generation: &count_generation,
                                            assets: treasure,
                                            aetheria_rate,
                                            root: random,
                                            epoch,
                                            operation,
                                            unix_seconds,
                                        },
                                    )
                                })
                                .transpose()?;
                            let count = match &olthoi {
                                Some(v) => 1 + v.sources.len(),
                                None => crate::player_death_preparation::death_identity_count(
                                    &count_items,
                                    &count_generation,
                                )?,
                            };
                            Ok::<_, String>((count, olthoi, None))
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r)
                        {
                            Ok(v) => v,
                            Err(e) => {
                                return Completion {
                                    identities,
                                    result: Err(e),
                                };
                            }
                        };
                    if identities.is_empty() && count != 0 {
                        match store.allocate_dynamic_ids(count as u16).await {
                            Ok(ids) => identities = ids.into_iter().map(EntityId).collect(),
                            Err(e) => {
                                return Completion {
                                    identities,
                                    result: Err(e.to_string()),
                                };
                            }
                        }
                    }
                    let ids = identities.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let mut assets =
                            crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
                        if let Some(loot) = no_corpse_loot.as_ref() {
                            let result = assets.prepare_player_no_corpse_death(
                                PlayerNoCorpsePreparationInput {
                                    operation,
                                    player: &saved,
                                    snapshot: &snapshot,
                                    items: &items,
                                    generation: &generation,
                                    geometry: &geometry,
                                    identities: &ids,
                                    loot,
                                    instantiation,
                                },
                            )?;
                            return Ok(Prepared {
                                command: PlayerDeathCommand::PrepareNoCorpse(Box::new(
                                    result.prepared,
                                )),
                                metadata: Metadata {
                                    fresh_items: result.fresh_items,
                                    positions: result.positions,
                                    appearance: result.appearance,
                                    corpse_visibility: None,
                                    world_visibility: result.world_visibility,
                                },
                            });
                        }
                        let result = assets.prepare_player_death(PlayerDeathPreparationInput {
                            operation,
                            player: &saved,
                            snapshot: &snapshot,
                            items: &items,
                            generation: &generation,
                            geometry: &geometry,
                            identities: &ids,
                            killer: killer.as_ref().map(|(id, name)| (*id, name.as_str())),
                            instantiation,
                            olthoi: olthoi.as_ref(),
                        })?;
                        Ok(Prepared {
                            command: PlayerDeathCommand::Prepare(Box::new(result.prepared)),
                            metadata: Metadata {
                                fresh_items: result.fresh_items,
                                positions: result.positions,
                                appearance: result.appearance,
                                corpse_visibility: Some(result.corpse_visibility),
                                world_visibility: vec![],
                            },
                        })
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r);
                    Completion { identities, result }
                }));
                p.phase = Phase::Cold;
            }
            Phase::Ready => {
                let correlation = self.token()?;
                let command = p.command.take().ok_or("death prepared command missing")?;
                match self
                    .simulation
                    .input()
                    .try_submit(Command::PlayerDeathService(PlayerDeathServiceCommand {
                        correlation,
                        command,
                    })) {
                    Ok(()) => p.phase = Phase::Submitted(correlation),
                    Err(error) => {
                        let (closed, command) = match error {
                            TrySendError::Full(c) => (false, c),
                            TrySendError::Disconnected(c) => (true, c),
                        };
                        let Command::PlayerDeathService(command) = command else {
                            unreachable!()
                        };
                        p.command = Some(command.command);
                        if closed {
                            return Err("death preparation owner closed".into());
                        }
                    }
                }
            }
            Phase::Proposal => {
                if let Some(ticket) = p.ticket.take() {
                    let metadata = p.cold.as_ref().ok_or("death prepared source missing")?;
                    let proof = p.proof.as_ref().ok_or("death initial capture missing")?;
                    let work = PlayerDeathWork {
                        epoch: self.bootstrap.world_owner.epoch(),
                        binding: p.binding,
                        ticket,
                        fresh_items: metadata.fresh_items.clone(),
                        positions: metadata.positions.clone(),
                    };
                    if let Err(work) =
                        self.deaths
                            .service
                            .stage_reserved(work, proof, &self.online_saves)
                    {
                        p.ticket = Some(work.ticket);
                        return Err("death save transfer rejected".into());
                    }
                    p.phase = Phase::Saving;
                }
            }
            Phase::Capturing(_) | Phase::Cold | Phase::Submitted(_) | Phase::Saving => {}
        }
        Ok(())
    }
}
