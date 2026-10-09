//! Authenticated corpse Use and the exact inspect → durable rights → adopt
//! transcript. One retained operation bounds source and DAT preparation work.
use super::*;
use crate::{
    game_runtime::progression::ProgressionIngress,
    placement_saves::{PendingPlacementSave, PlacementResolution},
    player_entry::PreparedEntryAppearanceAssets,
    region_unload_saves::RegionItemSource,
    saves::SaveSubmitError,
};
use bace_gameplay_api::ActionContext;
use bace_simulation::{CorpseAccessCommand, CorpseAccessDecision, CorpseAccessOutcome};
use bace_wire::{GameActionEnvelope, InventoryAction, InventoryRequest};
mod output;

pub(super) struct Pending {
    pub(super) key: SessionKey,
    pub(super) binding: CharacterBinding,
    context: ActionContext,
    pub(super) corpse: EntityId,
    source: RegionItemSource,
    children: Vec<RegionItemSource>,
    grandchildren: Vec<RegionItemSource>,
    appearance: Option<PreparedEntryAppearanceAssets>,
    appearance_job: Option<Job<Result<PreparedEntryAppearanceAssets, String>>>,
    source_refreshes: u8,
    inspection: u64,
    decision: Option<CorpseAccessDecision>,
    has_loot_permit: bool,
    rejection: Option<bace_simulation::CorpseAccessError>,
    phase: Phase,
    detaching: bool,
    followup: Option<Box<Pending>>,
}
enum Phase {
    Inspect,
    Inspecting,
    Saving {
        save: Box<PendingPlacementSave>,
        after: Box<bace_storage_codec::CorpseSaveV5>,
        submitted: bool,
    },
    Cache {
        after: Box<bace_storage_codec::CorpseSaveV5>,
        version: i64,
    },
    Adopt,
    Adopting(u64),
    Present,
    Blocked(String),
}

fn same_source_revision(before: &RegionItemSource, current: &RegionItemSource) -> bool {
    before.item.persisted_version == current.item.persisted_version
        && before.item.corpse == current.item.corpse
        && before.item.entity == current.item.entity
        && before.item.placement == current.item.placement
        && before.item.enchantments == current.item.enchantments
        && before.item.construction == current.item.construction
        && before.item.source_destination == current.item.source_destination
        && before.corpse == current.corpse
}

fn same_source_list(before: &[RegionItemSource], current: &[RegionItemSource]) -> bool {
    before.len() == current.len()
        && before
            .iter()
            .zip(current)
            .all(|(old, new)| same_source_revision(old, new))
}
impl GameRuntime {
    pub(in crate::game_runtime) fn handle_corpse_use_message(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<ProgressionIngress, String> {
        let Some(session) = self.sessions.get(&key) else {
            return Ok(ProgressionIngress::Blocked);
        };
        let Some(loading) = session.loading.as_ref() else {
            return Ok(ProgressionIngress::Unsupported);
        };
        let binding = loading.loaded.binding;
        if !self.players.entered(binding.actor) {
            return Ok(ProgressionIngress::Unsupported);
        }
        let envelope = match GameActionEnvelope::decode(&message.bytes, self.limits.message_bytes) {
            Ok(value) => value,
            Err(bace_wire::WireError::UnexpectedOpcode(_)) => {
                return Ok(ProgressionIngress::Unsupported);
            }
            Err(error) => return Err(format!("corpse Use envelope: {error:?}")),
        };
        if envelope.action != bace_wire::opcode::GameActionType::Use {
            return Ok(ProgressionIngress::Unsupported);
        }
        let request = InventoryRequest::decode(
            envelope.action,
            envelope.payload,
            self.limits.message_bytes,
            0,
        )
        .map_err(|error| format!("corpse Use action: {error:?}"))?;
        let InventoryAction::Use(raw) = request.action else {
            return Ok(ProgressionIngress::Unsupported);
        };
        let corpse = EntityId(raw);
        let Some(world) = self.world.as_ref() else {
            return Ok(ProgressionIngress::Blocked);
        };
        let Some(source) = world.regions.corpse_source(corpse).cloned() else {
            return Ok(ProgressionIngress::Unsupported);
        };
        if self.deaths.access.is_some() {
            return Ok(ProgressionIngress::Blocked);
        }
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("corpse Use authenticated owner mismatch".into());
        }
        let (children, grandchildren) = world.regions.corpse_open_contents(corpse)?;
        let appearance_job = self.corpse_appearance_job(&children, &grandchildren);
        let next = Pending {
            key,
            binding,
            context: ActionContext {
                actor: binding.actor,
                account: binding.account,
                session: binding.session,
                sequence: message.sequence,
            },
            corpse,
            source,
            children,
            grandchildren,
            appearance: None,
            appearance_job,
            source_refreshes: 0,
            inspection: 0,
            decision: None,
            has_loot_permit: false,
            rejection: None,
            phase: Phase::Inspect,
            detaching: false,
            followup: None,
        };
        if let Some((prior, prior_binding)) = self.deaths.viewers.get(&key).copied()
            && prior != corpse
        {
            let prior_source = self
                .world
                .as_ref()
                .and_then(|world| world.regions.corpse_source(prior))
                .cloned()
                .ok_or("previous corpse viewer source missing")?;
            let mut close = Pending {
                key,
                binding: prior_binding,
                context: next.context,
                corpse: prior,
                source: prior_source,
                children: Vec::new(),
                grandchildren: Vec::new(),
                appearance: None,
                appearance_job: None,
                source_refreshes: 0,
                inspection: 0,
                decision: None,
                has_loot_permit: false,
                rejection: None,
                phase: Phase::Inspect,
                detaching: false,
                followup: Some(Box::new(next)),
            };
            self.begin_corpse_close(&mut close)?;
            self.deaths.access = Some(close);
        } else {
            self.deaths.access = Some(next);
        }
        Ok(ProgressionIngress::Accepted)
    }

    pub(in crate::game_runtime) fn corpse_access_ingress_blocked(&self, key: SessionKey) -> bool {
        self.deaths
            .access
            .as_ref()
            .is_some_and(|pending| pending.key == key)
    }

    /// Logout calls this before DetachPlayer. False retains the logout until a
    /// durable close and its simulation receipt have finished.
    pub(in crate::game_runtime) fn queue_corpse_viewer_detach(
        &mut self,
        key: SessionKey,
    ) -> Result<bool, String> {
        if let Some(pending) = self.deaths.access.as_mut() {
            if pending.key == key {
                pending.detaching = true;
                pending.followup = None;
                return Ok(false);
            }
            return Ok(!self.deaths.viewers.contains_key(&key));
        }
        let Some((corpse, binding)) = self.deaths.viewers.get(&key).copied() else {
            return Ok(true);
        };
        let source = self
            .world
            .as_ref()
            .and_then(|world| world.regions.corpse_source(corpse))
            .cloned()
            .ok_or("disconnect corpse source missing")?;
        let mut pending = Pending {
            key,
            binding,
            context: ActionContext {
                actor: binding.actor,
                account: binding.account,
                session: binding.session,
                sequence: 0,
            },
            corpse,
            source,
            children: Vec::new(),
            grandchildren: Vec::new(),
            appearance: None,
            appearance_job: None,
            source_refreshes: 0,
            inspection: 0,
            decision: None,
            has_loot_permit: false,
            rejection: None,
            phase: Phase::Inspect,
            detaching: true,
            followup: None,
        };
        self.begin_corpse_close(&mut pending)?;
        self.deaths.access = Some(pending);
        Ok(false)
    }

    fn begin_corpse_close(&mut self, pending: &mut Pending) -> Result<(), String> {
        let mark_looted = !pending
            .source
            .corpse
            .as_ref()
            .ok_or("corpse close source missing")?
            .access
            .looted;
        let decision = CorpseAccessDecision::Close { mark_looted };
        pending.inspection = self.token()?;
        pending.decision = Some(decision);
        pending.children.clear();
        pending.grandchildren.clear();
        pending.appearance = None;
        pending.appearance_job = None;
        let frozen = access_saves::freeze(
            self.bootstrap.world_owner.epoch(),
            pending.inspection,
            pending.corpse,
            pending.binding.actor,
            decision,
            &pending.source,
        )
        .map_err(|error| error.to_string())?;
        pending.phase = if let Some((operation, after)) = frozen {
            Phase::Saving {
                save: Box::new(
                    PendingPlacementSave::new_world(operation)
                        .map_err(|error| error.to_string())?,
                ),
                after: Box::new(after),
                submitted: false,
            }
        } else {
            Phase::Adopt
        };
        Ok(())
    }

    fn corpse_appearance_job(
        &self,
        children: &[RegionItemSource],
        grandchildren: &[RegionItemSource],
    ) -> Option<Job<Result<PreparedEntryAppearanceAssets, String>>> {
        if children.is_empty() && grandchildren.is_empty() {
            return None;
        }
        let manifest = self.bootstrap.assets.clone();
        let sources = children
            .iter()
            .chain(grandchildren.iter())
            .map(|item| item.item.entity.state.clone())
            .collect::<Vec<_>>();
        Some(Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
                assets.prepare_entry_appearance(&sources.iter().collect::<Vec<_>>())
            })
            .await
            .map_err(|error| format!("corpse appearance worker: {error}"))?
        }))
    }

    /// A committed inventory mutation may finish while the corpse's DAT work
    /// is pending. Present only the latest accepted source versions. The
    /// simulation viewer was adopted already, so refreshing presentation does
    /// not repeat or consume a durable loot permit.
    fn refresh_corpse_open_sources(&self, pending: &mut Pending) -> Result<bool, String> {
        if !matches!(pending.decision, Some(CorpseAccessDecision::Open { .. })) {
            return Ok(true);
        }
        let regions = &self
            .world
            .as_ref()
            .ok_or("corpse open region missing")?
            .regions;
        let current = regions
            .corpse_source(pending.corpse)
            .ok_or("corpse open source retired before presentation")?;
        if !same_source_revision(&pending.source, current) {
            return Err("corpse open root source changed after adoption".into());
        }
        let (children, grandchildren) = regions.corpse_open_contents(pending.corpse)?;
        if same_source_list(&pending.children, &children)
            && same_source_list(&pending.grandchildren, &grandchildren)
        {
            return Ok(true);
        }
        if pending.source_refreshes >= 8 {
            return Err("corpse open source changed repeatedly during presentation".into());
        }
        pending.source_refreshes += 1;
        pending.appearance = None;
        pending.appearance_job = self.corpse_appearance_job(&children, &grandchildren);
        pending.children = children;
        pending.grandchildren = grandchildren;
        Ok(false)
    }

    pub(super) fn poll_corpse_access(&mut self, unix_millis: u64) -> Result<(), String> {
        if self.deaths.unexpected_access.is_some() {
            return Err("unmatched corpse access outcome retained".into());
        }
        let Some(mut pending) = self.deaths.access.take() else {
            return if let Ok(outcome) = self.simulation.corpse_access_outcomes().try_recv() {
                self.deaths.unexpected_access = Some(outcome);
                Err("unmatched corpse access outcome retained".into())
            } else {
                Ok(())
            };
        };
        if let Some(appearance) = ready(&mut pending.appearance_job) {
            pending.appearance = Some(appearance?);
        }
        let phase = std::mem::replace(&mut pending.phase, Phase::Inspect);
        let result = self.advance_corpse_access(&mut pending, phase, unix_millis / 1000);
        match result {
            Ok(true) => {
                self.deaths.access = pending.followup.take().map(|next| *next);
                Ok(())
            }
            Ok(false) => {
                self.deaths.access = Some(pending);
                Ok(())
            }
            Err(error) => {
                if !matches!(pending.phase, Phase::Saving { .. } | Phase::Cache { .. }) {
                    pending.phase = Phase::Blocked(error.clone());
                }
                self.deaths.access = Some(pending);
                Err(error)
            }
        }
    }

    fn advance_corpse_access(
        &mut self,
        p: &mut Pending,
        phase: Phase,
        unix_seconds: u64,
    ) -> Result<bool, String> {
        match phase {
            Phase::Inspect => {
                let correlation = self.token()?;
                let command = Command::CorpseAccess(CorpseAccessCommand::Inspect {
                    correlation,
                    context: p.context,
                    corpse: p.corpse,
                    unix_seconds,
                });
                match self.simulation.input().try_submit(command) {
                    Ok(()) => {
                        p.inspection = correlation;
                        p.phase = Phase::Inspecting;
                    }
                    Err(TrySendError::Full(_)) => p.phase = Phase::Inspect,
                    Err(TrySendError::Disconnected(_)) => {
                        return Err("corpse inspect owner closed".into());
                    }
                }
            }
            Phase::Inspecting => {
                let Ok(outcome) = self.simulation.corpse_access_outcomes().try_recv() else {
                    p.phase = Phase::Inspecting;
                    return Ok(false);
                };
                let retained = outcome.clone();
                let CorpseAccessOutcome::Inspected {
                    correlation,
                    actor,
                    corpse,
                    result,
                } = outcome
                else {
                    self.deaths.unexpected_access = Some(retained);
                    return Err("corpse inspect outcome phase mismatch".into());
                };
                if correlation != p.inspection || actor != p.binding.actor || corpse != p.corpse {
                    self.deaths.unexpected_access = Some(retained);
                    return Err("corpse inspect outcome identity mismatch".into());
                }
                let inspected = match result {
                    Ok(value) => value,
                    Err(error) => {
                        p.rejection = Some(error);
                        p.phase = Phase::Present;
                        return Ok(false);
                    }
                };
                let saved = p
                    .source
                    .corpse
                    .as_ref()
                    .ok_or("corpse source profile absent")?;
                if inspected.operation != saved.operation.unwrap_or(0)
                    || inspected.profile.victim.map(|id| id.0) != saved.access.victim
                    || inspected.profile.killer.map(|id| id.0) != saved.access.killer
                    || inspected.profile.is_monster != saved.access.is_monster
                    || inspected.profile.generated_rare != saved.access.generated_rare
                    || inspected.profile.pk_death != saved.access.pk_death
                    || inspected.profile.looted != saved.access.looted
                    || inspected
                        .profile
                        .permittees
                        .iter()
                        .map(|id| id.0)
                        .collect::<Vec<_>>()
                        != saved.access.permittees
                {
                    return Err("corpse inspect source revision mismatch".into());
                }
                p.decision = Some(inspected.decision);
                p.has_loot_permit = inspected.has_loot_permit;
                if matches!(inspected.decision, CorpseAccessDecision::Denied(_)) {
                    p.phase = Phase::Present;
                } else {
                    let frozen = access_saves::freeze(
                        self.bootstrap.world_owner.epoch(),
                        p.inspection,
                        p.corpse,
                        p.binding.actor,
                        inspected.decision,
                        &p.source,
                    )
                    .map_err(|error| error.to_string())?;
                    p.phase = if let Some((operation, after)) = frozen {
                        Phase::Saving {
                            save: Box::new(
                                PendingPlacementSave::new_world(operation)
                                    .map_err(|error| error.to_string())?,
                            ),
                            after: Box::new(after),
                            submitted: false,
                        }
                    } else {
                        Phase::Adopt
                    };
                }
            }
            Phase::Saving {
                mut save,
                after,
                mut submitted,
            } => {
                if !submitted {
                    match save.submit(&self.saves.handle) {
                        Ok(()) => submitted = true,
                        Err(SaveSubmitError::Full) => {}
                        Err(error) => return Err(format!("corpse access save submit: {error}")),
                    }
                    p.phase = Phase::Saving {
                        save,
                        after,
                        submitted,
                    };
                    return Ok(false);
                }
                match save.poll() {
                    Some(PlacementResolution::Committed(acks)) => {
                        let ack = acks
                            .iter()
                            .find(|ack| ack.object_id == p.corpse.0)
                            .ok_or("corpse access receipt missing")?;
                        p.phase = Phase::Cache {
                            after,
                            version: ack.persisted_version,
                        };
                    }
                    Some(PlacementResolution::Rejected(error)) => {
                        p.phase = Phase::Blocked(format!("corpse access save rejected: {error}"));
                        return Err(format!("corpse access save rejected: {error}"));
                    }
                    Some(PlacementResolution::Uncertain(error)) => {
                        p.phase = Phase::Saving {
                            save,
                            after,
                            submitted: false,
                        };
                        return Err(format!("corpse access save uncertain: {error}"));
                    }
                    None => {
                        p.phase = Phase::Saving {
                            save,
                            after,
                            submitted,
                        };
                    }
                }
            }
            Phase::Cache { after, version } => {
                let world = self.world.as_mut().ok_or("corpse access region missing")?;
                if let Err(error) = world.regions.replace_corpse_source(
                    p.corpse,
                    p.source.item.persisted_version,
                    *after.clone(),
                    version,
                ) {
                    p.phase = Phase::Cache { after, version };
                    return Err(error);
                }
                p.source.item.persisted_version = version;
                p.source.item.entity = after.corpse.entity.clone();
                p.source.item.placement = Some(after.placement.clone());
                p.source.item.enchantments = after.enchantments.clone();
                p.source.item.corpse = Some(Box::new(*after.clone()));
                p.source.corpse = Some(*after);
                p.phase = Phase::Adopt;
            }
            Phase::Adopt => {
                let correlation = self.token()?;
                let command = Command::CorpseAccess(CorpseAccessCommand::Adopt {
                    correlation,
                    context: p.context,
                    corpse: p.corpse,
                    has_loot_permit: p.has_loot_permit,
                    decision: p.decision.ok_or("corpse access decision missing")?,
                });
                match self.simulation.input().try_submit(command) {
                    Ok(()) => p.phase = Phase::Adopting(correlation),
                    Err(TrySendError::Full(_)) => p.phase = Phase::Adopt,
                    Err(TrySendError::Disconnected(_)) => {
                        return Err("corpse adopt owner closed".into());
                    }
                }
            }
            Phase::Adopting(correlation) => {
                let Ok(outcome) = self.simulation.corpse_access_outcomes().try_recv() else {
                    p.phase = Phase::Adopting(correlation);
                    return Ok(false);
                };
                let retained = outcome.clone();
                let CorpseAccessOutcome::Adopted {
                    correlation: got,
                    actor,
                    corpse,
                    result,
                } = outcome
                else {
                    self.deaths.unexpected_access = Some(retained);
                    return Err("corpse adoption outcome phase mismatch".into());
                };
                if got != correlation || actor != p.binding.actor || corpse != p.corpse {
                    self.deaths.unexpected_access = Some(retained);
                    return Err("corpse adoption outcome identity mismatch".into());
                }
                if result.map_err(|error| format!("corpse adoption: {error:?}"))?
                    != p.decision.ok_or("corpse adopted decision missing")?
                {
                    return Err("corpse adopted decision mismatch".into());
                }
                p.phase = Phase::Present;
            }
            Phase::Present => {
                if p.detaching && matches!(p.decision, Some(CorpseAccessDecision::Open { .. })) {
                    self.begin_corpse_close(p)?;
                    return Ok(false);
                }
                if p.detaching {
                    if self
                        .deaths
                        .viewers
                        .get(&p.key)
                        .is_some_and(|viewer| *viewer != (p.corpse, p.binding))
                    {
                        return Err("corpse detach exact viewer binding mismatch".into());
                    }
                    self.deaths.viewers.remove(&p.key);
                    return Ok(true);
                }
                if p.appearance_job.is_some() {
                    p.phase = Phase::Present;
                    return Ok(false);
                }
                if !self.refresh_corpse_open_sources(p)? {
                    p.phase = Phase::Present;
                    return Ok(false);
                }
                if self.network_output.len() >= self.limits.messages {
                    p.phase = Phase::Present;
                    return Ok(false);
                }
                if self.project_corpse_access(p)? {
                    return Ok(true);
                }
                p.phase = Phase::Present;
            }
            Phase::Blocked(error) => {
                p.phase = Phase::Blocked(error.clone());
                return Err(error);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content::WeenieV1;
    use bace_storage_codec::{EntitySaveV1, ItemPlacementV2};

    fn child(id: u32, version: i64, parent: u32, slot: u32) -> RegionItemSource {
        RegionItemSource {
            item: crate::game_inventory::FrozenInventoryItem {
                corpse: None,
                construction: None,
                source_destination: Some(1),
                enchantments: vec![],
                entity: EntitySaveV1 {
                    object_id: id,
                    template_revision: 1,
                    mutation_revision: version as u64,
                    state: WeenieV1 {
                        schema_version: 1,
                        weenie_id: 100,
                        class_name: "nested corpse item".into(),
                        weenie_type: 1,
                        last_modified: None,
                        properties: Default::default(),
                    },
                },
                placement: Some(ItemPlacementV2::Contained {
                    container: parent,
                    slot,
                    pack_slot: false,
                    equipped: 0,
                }),
                persisted_version: version,
            },
            corpse: None,
        }
    }

    #[test]
    fn corpse_open_rechecks_nested_committed_version_and_placement() {
        let before = [child(0x8000_0002, 5, 0x8000_0001, 0)];
        assert!(same_source_list(&before, &before));
        assert!(!same_source_list(
            &before,
            &[child(0x8000_0002, 6, 0x8000_0001, 0)]
        ));
        assert!(!same_source_list(
            &before,
            &[child(0x8000_0002, 5, 0x8000_0001, 1)]
        ));
        let mut mismatched_payload = before[0].clone();
        mismatched_payload.item.entity.state.class_name = "unreceipted change".into();
        assert!(!same_source_list(&before, &[mismatched_payload]));
        assert!(!same_source_list(&before, &[]));
    }
}
