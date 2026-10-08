//! Joint frozen stage preparation after the exact owner checkpoint and player
//! snapshot. Pure source stages do not manufacture a player aggregate.
use super::effects::{Owner, Phase};
use super::*;
use crate::{npc_persistence::*, npc_service::NpcDurableAdoption};
use bace_simulation::{PlayerSnapshotOperation, PlayerSnapshotRequest};
impl GameRuntime {
    pub(super) fn poll_npc_ready(&mut self, source: EntityId, token: u64) -> Result<(), String> {
        let work = self
            .npc
            .work
            .get_mut(&source)
            .ok_or("NPC prepared stage work missing")?;

        if matches!(work.owner, Owner::Cast(_) | Owner::Motion(_)) {
            let adoption = match &work.owner {
                Owner::Cast(outcome) => NpcDurableAdoption::Cast {
                    proposal: work.proposal.clone(),
                    outcome: outcome.clone(),
                },
                Owner::Motion(event) => NpcDurableAdoption::Motion {
                    proposal: work.proposal.clone(),
                    event: *event,
                },
                _ => return Err("NPC terminal service mismatch".into()),
            };
            let (binding, workflow_version) = self
                .npc
                .coordinator
                .binding(source)
                .ok_or("NPC cast journal binding")?;
            let leases = work
                .proposal
                .context
                .target
                .and_then(|actor| {
                    self.online_saves
                        .baseline(actor.0)
                        .map(|(_, _, lease)| lease)
                })
                .into_iter()
                .collect();
            let pending = freeze_service_completion(NpcServiceCompletionInput {
                binding,
                workflow_version,
                world_epoch: self.bootstrap.world_owner.epoch(),
                proposal: work.proposal.clone(),
                checkpoint: work
                    .checkpoint
                    .as_ref()
                    .ok_or("NPC cast completion preview")?
                    .clone(),
                leases,
            })
            .map_err(|e| e.to_string())?;
            work.frozen = Some((pending, adoption));
            work.phase = Phase::Saving;
            return Ok(());
        }
        if matches!(
            work.proposal.effect,
            NpcEffect::Property {
                aggregate: None,
                ..
            } | NpcEffect::Quest {
                aggregate: None,
                ..
            }
        ) {
            let (binding, workflow_version) = self
                .npc
                .coordinator
                .binding(source)
                .ok_or("NPC archive binding missing")?;
            let leases = work
                .proposal
                .context
                .target
                .and_then(|actor| {
                    self.online_saves
                        .baseline(actor.0)
                        .map(|(_, _, lease)| lease)
                })
                .into_iter()
                .collect();
            let pending = freeze_archive_stage(NpcArchiveStageInput {
                binding,
                world_epoch: self.bootstrap.world_owner.epoch(),
                workflow_version,
                proposal: work.proposal.clone(),
                committed_checkpoint: work
                    .checkpoint
                    .as_ref()
                    .ok_or("NPC archive property preview missing")?
                    .clone(),
                leases,
            })
            .map_err(|e| e.to_string())?;
            work.frozen = Some((pending, NpcDurableAdoption::Player));
            work.phase = Phase::Saving;
            return Ok(());
        }
        if let Owner::Delete(ticket) = &work.owner {
            if let Some(retirement) = &ticket.retirement
                && work.deletion_sources.is_none()
            {
                if work.deletion_loading || self.npc.deletion_job.is_some() {
                    return Ok(());
                }
                let Some(world) = &self.world else {
                    return Ok(());
                };
                let transient = retirement
                    .transient
                    .iter()
                    .map(|id| {
                        world
                            .regions
                            .transient_source(*id)
                            .map(|s| s.item.clone())
                            .ok_or("NPC transient item metadata missing")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let retirement = retirement.clone();
                let ticket_id = work.proposal.ticket;
                let store = self.bootstrap.store.clone();
                work.deletion_loading = true;
                self.npc.deletion_job = Some(Box::pin(async move {
                    let result = prepare_deletion_sources(&store, &retirement, transient).await;
                    (source, ticket_id, result)
                }));
                return Ok(());
            }
            let (binding, workflow_version) = self
                .npc
                .coordinator
                .binding(source)
                .ok_or("NPC deletion source binding missing")?;
            let leases = work
                .proposal
                .context
                .target
                .and_then(|actor| {
                    self.online_saves
                        .baseline(actor.0)
                        .map(|(_, _, lease)| lease)
                })
                .into_iter()
                .collect();
            let positions = BTreeMap::new();
            let frozen = freeze_delete_source_stage(NpcDeleteSourceStageInput {
                binding,
                world_epoch: self.bootstrap.world_owner.epoch(),
                workflow_version,
                ticket,
                committed_checkpoint: work
                    .checkpoint
                    .as_ref()
                    .ok_or("NPC archive preview missing")?
                    .clone(),
                retirement: ticket.retirement.as_ref().map(|retirement| {
                    crate::game_inventory::InventoryFreezeInput {
                        operation_id: "npc-retirement-replaced",
                        proposal: &retirement.inventory.proposal,
                        items: work.deletion_sources.as_deref().unwrap_or(&[]),
                        other_snapshots: &[],
                        leases: &[],
                        storage_views: &[],
                        admitted_positions: &positions,
                    }
                }),
                leases,
            })
            .map_err(|e| e.to_string())?;
            work.frozen = Some((
                frozen.pending,
                NpcDurableAdoption::Delete {
                    ticket: ticket.clone(),
                    receipt: frozen.receipt,
                },
            ));
            work.phase = Phase::Saving;
            return Ok(());
        }
        if let NpcEffect::QueuedExperience {
            actor,
            phase: bace_simulation::NpcQueuedExperiencePhase::AwaitingAdmission,
            ..
        } = work.proposal.effect
        {
            let (_, _, lease) = self
                .online_saves
                .baseline(actor.0)
                .ok_or("NPC XP target lease missing")?;
            let (binding, workflow_version) = self
                .npc
                .coordinator
                .binding(source)
                .ok_or("NPC XP source binding missing")?;
            let pending = freeze_experience_admission(NpcExperienceAdmissionInput {
                binding,
                stage: workflow_version as u64,
                world_epoch: self.bootstrap.world_owner.epoch(),
                workflow_version,
                proposal: work.proposal.clone(),
                committed_checkpoint: work
                    .checkpoint
                    .as_ref()
                    .ok_or("NPC XP preview missing")?
                    .clone(),
                lease,
            })
            .map_err(|e| e.to_string())?;
            work.frozen = Some((pending, NpcDurableAdoption::Player));
            work.phase = Phase::Saving;
            return Ok(());
        }
        let (actor, revision) = work
            .actor_revision()
            .ok_or("NPC effect lacks player aggregate owner")?;
        if work.critical.is_none() {
            if !self.online_saves.critical_ready(&[actor.0])? {
                return Ok(());
            }
            self.online_saves.begin_critical(&[actor.0])?;
            work.critical = Some(actor.0);
        }
        if work.snapshot.is_none() {
            if work.capture.is_some() {
                return Ok(());
            }
            let binding = self
                .sessions
                .values()
                .filter_map(|s| s.loading.as_ref())
                .find(|l| l.loaded.binding.actor == actor)
                .map(|l| l.loaded.binding)
                .ok_or("NPC target is not admitted by a player owner")?;
            let command = bace_simulation::Command::PlayerSnapshot(PlayerSnapshotRequest {
                correlation: token,
                binding,
                operation: Some((
                    PlayerSnapshotOperation::Npc {
                        ticket: work.proposal.ticket,
                    },
                    revision,
                )),
            });
            if self.simulation.input().try_submit(command).is_ok() {
                work.capture = Some(token);
            }
            return Ok(());
        }
        let (snapshot, unix) = work.snapshot.as_ref().expect("captured NPC baseline");
        let (baseline, version, lease) = self
            .online_saves
            .baseline(actor.0)
            .ok_or("NPC durable baseline missing")?;
        let player = crate::player_saves::freeze_player_operation_baseline(
            baseline,
            snapshot,
            PlayerSnapshotOperation::Npc {
                ticket: work.proposal.ticket,
            },
            revision,
            *unix,
        )
        .map_err(|e| e.to_string())?;
        let (binding, workflow_version) = self
            .npc
            .coordinator
            .binding(source)
            .ok_or("NPC source journal binding missing")?;
        let checkpoint = work
            .checkpoint
            .as_ref()
            .ok_or("NPC checkpoint preview missing")?
            .clone();
        let completion = checkpoint
            .pending
            .iter()
            .find(|p| p.proposal == work.proposal)
            .ok_or("NPC adopted checkpoint row missing")?
            .completion;
        let world_epoch = self.bootstrap.world_owner.epoch();
        let (mut pending, adoption) = match &work.owner {
            Owner::Cast(_) | Owner::Motion(_) => {
                return Err("NPC terminal service player path mismatch".into());
            }
            Owner::Portal(ticket) => {
                let frozen = freeze_teleport_stage(NpcTeleportStageInput {
                    binding,
                    world_epoch,
                    workflow_version,
                    ticket,
                    committed_checkpoint: checkpoint,
                    players: &[crate::portal_saves::PortalSavePlayer {
                        saved: &player,
                        version,
                        lease,
                    }],
                })
                .map_err(|e| e.to_string())?;
                (
                    frozen.pending,
                    NpcDurableAdoption::Teleport {
                        ticket: ticket.clone(),
                        receipt: frozen.receipt,
                    },
                )
            }
            Owner::Inventory(ticket) => {
                let mut items = self.online_saves.operation_inventory_baselines(snapshot)?;
                if let Some(super::giving::Gift::Built(gift)) = &work.gift {
                    items.extend(gift.frozen.clone());
                }
                let others = [bace_persistence::SaveSnapshot {
                    object_id: actor.0,
                    mutation_revision: player.player.entity.mutation_revision,
                    expected_version: version,
                    bytes: player.encode().map_err(|e| e.to_string())?,
                }];
                let positions = BTreeMap::new();
                let pending = freeze_inventory_stage(NpcInventoryStageInput {
                    binding,
                    stage: workflow_version as u64,
                    world_epoch,
                    workflow_version,
                    ticket,
                    committed_checkpoint: checkpoint,
                    inventory: crate::game_inventory::InventoryFreezeInput {
                        operation_id: "npc-inventory-replaced",
                        proposal: &ticket.inventory.proposal,
                        items: &items,
                        other_snapshots: &others,
                        leases: &[lease],
                        storage_views: &[],
                        admitted_positions: &positions,
                    },
                })
                .map_err(|e| e.to_string())?;
                let receipt = bace_simulation::InventoryReceipt {
                    operation: ticket.inventory.operation,
                    revisions: ticket
                        .inventory
                        .proposal
                        .changes
                        .iter()
                        .map(|c| (c.after.id, c.after.revision))
                        .collect(),
                };
                (
                    pending,
                    NpcDurableAdoption::Inventory {
                        ticket: ticket.clone(),
                        receipt,
                    },
                )
            }
            Owner::Delete(_) => return Err("NPC deletion player path mismatch".into()),
            Owner::Player => (
                freeze_player_stage(NpcPlayerStageInput {
                    binding,
                    stage: workflow_version as u64,
                    world_epoch,
                    workflow_version,
                    proposal: work.proposal.clone(),
                    completion,
                    committed_checkpoint: checkpoint,
                    player: &player,
                    player_version: version,
                    lease,
                })
                .map_err(|e| e.to_string())?,
                NpcDurableAdoption::Player,
            ),
            Owner::Credits(ticket) => (
                freeze_training_credit_stage(NpcTrainingCreditStageInput {
                    binding,
                    world_epoch,
                    workflow_version,
                    ticket,
                    committed_checkpoint: checkpoint,
                    player: &player,
                    player_version: version,
                    lease,
                })
                .map_err(|e| e.to_string())?,
                NpcDurableAdoption::Credits(ticket.clone()),
            ),
            Owner::Book(ticket) => (
                freeze_spellbook_stage(NpcSpellbookStageInput {
                    binding,
                    world_epoch,
                    workflow_version,
                    ticket,
                    committed_checkpoint: checkpoint,
                    player: &player,
                    player_version: version,
                    lease,
                })
                .map_err(|e| e.to_string())?,
                NpcDurableAdoption::Spellbook(ticket.clone()),
            ),
            Owner::Skill(ticket) => (
                freeze_skill_reset_stage(NpcSkillResetStageInput {
                    binding,
                    world_epoch,
                    workflow_version,
                    ticket,
                    committed_checkpoint: checkpoint,
                    player: &player,
                    player_version: version,
                    lease,
                })
                .map_err(|e| e.to_string())?,
                NpcDurableAdoption::SkillReset(ticket.clone()),
            ),
        };
        // Keep the exact committed bytes until the actual owner reply;
        // baseline release must not race routine snapshots or logout.
        let mut captured = self.online_saves.operation_inventory_changes(snapshot)?;
        if let Owner::Inventory(ticket) = &work.owner {
            captured.retain(|row| {
                !ticket
                    .inventory
                    .proposal
                    .changes
                    .iter()
                    .any(|c| c.after.id.0 == row.object_id)
            });
        }
        pending
            .join_captured_inventory(captured)
            .map_err(|e| e.to_string())?;
        work.rows = pending.operation().inventory.snapshots.clone();
        for row in &mut work.rows {
            row.expected_version += 1;
        }
        work.frozen = Some((pending, adoption));
        work.phase = Phase::Saving;

        Ok(())
    }
}
