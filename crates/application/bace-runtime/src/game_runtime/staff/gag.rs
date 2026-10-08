//! Exact retained gag operation. Offline writes share login's lease fence.
use super::*;
use bace_gameplay_api::{social::SocialIdentity, staff::StaffAction, staff_gags::StaffGagProposal};
use bace_persistence::{SaveSnapshot, StaffGagOperation, StaffGagReceipt};
use bace_simulation::{
    Command, PlayerSnapshotOperation, PlayerSnapshotOutcome, PlayerSnapshotRequest,
};
use bace_storage_codec::PlayerSaveV6;
pub(in crate::game_runtime) struct GagLookup {
    name: String,
    resolved: Option<Option<bace_db_postgres::PlayerIdentity>>,
    sudo: bool,
    enabled: bool,
    job: Job<Result<Option<bace_db_postgres::PlayerIdentity>, bace_db_postgres::StoreError>>,
}
pub(super) struct GagPending {
    pub proposal: StaffGagProposal,
    phase: GagPhase,
}
enum GagPhase {
    Barrier,
    CaptureReady,
    Capturing(u64),
    Captured(Arc<bace_simulation::PlayerReadSnapshot>, u64),
    Offline(Job<Result<bace_persistence::OfflineStaffPlayer, bace_db_postgres::StoreError>>),
    Loaded(bace_persistence::OfflineStaffPlayer),
    Write {
        operation: Box<StaffGagOperation>,
        uncertain: bool,
    },
    Writing {
        operation: Box<StaffGagOperation>,
        uncertain: bool,
        job: Job<Result<StaffGagReceipt, bace_db_postgres::StoreError>>,
    },
    Owner {
        row: Option<SaveSnapshot>,
        queued: bool,
    },
}
impl GagPending {
    pub(super) fn new(proposal: StaffGagProposal) -> Self {
        Self {
            proposal,
            phase: GagPhase::Barrier,
        }
    }
    pub(super) fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        matches!(self.phase,GagPhase::Capturing(c) if c==outcome.correlation)
    }
}
fn poll<T>(job: &mut Job<T>) -> Option<T> {
    match job.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => Some(v),
        Poll::Pending => None,
    }
}
impl GameRuntime {
    pub(super) fn poll_staff_gag_lookup(&mut self) -> Result<(), String> {
        let Some(p) = &mut self.staff.pending else {
            return Ok(());
        };
        if let Phase::Other { command, .. } = &p.phase {
            let Some(bace_admin::StaffOperation::Gag { name, seconds }) =
                bace_admin::prepare_staff_operation(command)
                    .map_err(|e| format!("gag arguments: {e:?}"))?
            else {
                return Ok(());
            };
            let store = self.bootstrap.store.clone();
            p.phase = Phase::GagLookup(GagLookup {
                name: name.clone(),
                resolved: None,
                sudo: command.sudo,
                enabled: seconds.is_some(),
                job: Box::pin(async move {
                    store
                        .lookup_player_identity(&bace_db_postgres::PlayerIdentityQuery::Name(name))
                        .await
                }),
            });
        }
        let Phase::GagLookup(lookup) = &mut p.phase else {
            return Ok(());
        };
        if lookup.resolved.is_none()
            && let Some(result) = poll(&mut lookup.job)
        {
            match result {
                Ok(identity) => lookup.resolved = Some(identity),
                Err(error) => {
                    let name = lookup.name.clone();
                    let store = self.bootstrap.store.clone();
                    lookup.job = Box::pin(async move {
                        store
                            .lookup_player_identity(&bace_db_postgres::PlayerIdentityQuery::Name(
                                name,
                            ))
                            .await
                    });
                    return Err(error.to_string());
                }
            }
        }
        if let Some(identity) = lookup.resolved.clone() {
            let identity = match identity {
                Some(identity) => identity,
                None => {
                    let verb = if lookup.enabled { "gag" } else { "ungag" };
                    let text = format!(
                        "Unable to {verb} a character named {}, check the name and re-try the command.",
                        lookup.name
                    );
                    let r = self
                        .players
                        .replication(p.context.actor)
                        .ok_or("gag issuer missing")?;
                    let batch = bace_replication::project_staff_response(
                        r.binding,
                        &text,
                        20,
                        bace_replication::BatchLimits {
                            max_messages: 1,
                            max_bytes: self.limits.message_bytes,
                            max_message_bytes: self.limits.message_bytes,
                            max_string_bytes: 4096,
                        },
                    )
                    .map_err(|e| format!("gag response: {e:?}"))?;
                    self.staff.output.push_back(
                        crate::game_messages::session_batch_command(r.key, batch)
                            .map_err(|e| e.to_string())?,
                    );
                    self.staff.pending = None;
                    return Ok(());
                }
            };
            let unix = self.clock.unix_millis as f64 / 1000. + self.last_elapsed.as_secs_f64();
            p.phase = Phase::Command(Box::new(StaffCommand {
                token: p.token,
                action: StaffAction::Gag {
                    context: p.context,
                    requested_name: lookup.name.clone(),
                    target: SocialIdentity {
                        character: bace_types::EntityId(identity.object_id),
                        account: bace_types::AccountId(identity.account_id),
                        name: identity.name,
                    },
                    enabled: lookup.enabled,
                    unix_seconds: unix,
                    sudo: lookup.sudo,
                },
            }));
        }
        Ok(())
    }
    pub(super) fn accept_staff_gag_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), String> {
        let p = self.staff.gag.as_mut().ok_or("missing gag capture")?;
        if !p.owns_capture(&outcome) {
            return Err("gag capture correlation".into());
        }
        match outcome.result {
            Ok(s) => p.phase = GagPhase::Captured(s, unix),
            Err(e) => {
                p.phase = GagPhase::CaptureReady;
                return Err(format!("gag capture: {e:?}"));
            }
        }
        Ok(())
    }
    pub(super) fn poll_staff_gag(&mut self) -> Result<(), String> {
        let correlation = self.token()?;
        let Some(p) = &mut self.staff.gag else {
            return Ok(());
        };
        let actor = p.proposal.target.character.0;
        match &mut p.phase {
            GagPhase::Barrier => {
                if p.proposal.before_revision.is_some() {
                    if self.online_saves.critical_ready(&[actor])? {
                        self.online_saves.begin_critical(&[actor])?;
                        p.phase = GagPhase::CaptureReady;
                    }
                } else {
                    let store = self.bootstrap.store.clone();
                    p.phase = GagPhase::Offline(Box::pin(async move {
                        store.load_offline_staff_player(actor).await
                    }));
                }
            }
            GagPhase::CaptureReady => {
                let binding = self
                    .players
                    .replication(p.proposal.target.character)
                    .ok_or("gag online binding missing")?
                    .binding;
                if binding.account != p.proposal.target.account {
                    return Err("gag account mismatch".into());
                }
                if self
                    .simulation
                    .input()
                    .try_submit(Command::PlayerSnapshot(PlayerSnapshotRequest {
                        correlation,
                        binding,
                        operation: Some((
                            PlayerSnapshotOperation::StaffGag(p.proposal.operation),
                            p.proposal.before_revision.expect("online gag"),
                        )),
                    }))
                    .is_ok()
                {
                    p.phase = GagPhase::Capturing(correlation);
                }
            }
            GagPhase::Captured(snapshot, unix) => {
                let (base, version, lease) = self
                    .online_saves
                    .baseline(actor)
                    .ok_or("gag baseline missing")?;
                let before = crate::player_saves::freeze_player_operation_baseline(
                    base,
                    snapshot,
                    PlayerSnapshotOperation::StaffGag(p.proposal.operation),
                    p.proposal.before_revision.expect("online gag"),
                    *unix,
                )
                .map_err(|e| e.to_string())?;
                p.phase = GagPhase::Write {
                    operation: Box::new(operation(
                        self.bootstrap.world_owner.epoch(),
                        &p.proposal,
                        &before,
                        version,
                        lease,
                    )?),
                    uncertain: false,
                };
            }
            GagPhase::Offline(job) => {
                if let Some(result) = poll(job) {
                    p.phase = match result {
                        Ok(loaded) => GagPhase::Loaded(loaded),
                        Err(_) => GagPhase::Owner {
                            row: None,
                            queued: false,
                        },
                    };
                }
            }
            GagPhase::Loaded(loaded) => {
                let before = PlayerSaveV6::decode_or_migrate(&loaded.snapshot.bytes)
                    .map_err(|e| e.to_string())?;
                let operation = operation(
                    self.bootstrap.world_owner.epoch(),
                    &p.proposal,
                    &before,
                    loaded.snapshot.persisted_version,
                    loaded.lease,
                )?;
                p.phase = GagPhase::Write {
                    operation: Box::new(operation),
                    uncertain: false,
                };
            }

            GagPhase::Write {
                operation,
                uncertain,
            } => {
                let store = self.bootstrap.store.clone();
                let request = operation.clone();
                p.phase = GagPhase::Writing {
                    operation: operation.clone(),
                    uncertain: *uncertain,
                    job: Box::pin(async move { store.apply_staff_gag(&request).await }),
                };
            }
            GagPhase::Writing {
                operation,
                uncertain,
                job,
            } => {
                if let Some(result) = poll(job) {
                    match result {
                        Ok(receipt)
                            if receipt.operation_id == operation.operation_id
                                && receipt.acknowledgement.object_id == actor
                                && receipt.acknowledgement.mutation_revision
                                    == operation.snapshot.mutation_revision
                                && receipt.acknowledgement.persisted_version
                                    == operation.snapshot.expected_version + 1 =>
                        {
                            let mut row = operation.snapshot.clone();
                            row.expected_version += 1;
                            p.phase = GagPhase::Owner {
                                row: Some(row),
                                queued: false,
                            };
                        }
                        Err(error)
                            if !*uncertain
                                && !matches!(
                                    error,
                                    bace_db_postgres::StoreError::CommitUncertain(_)
                                ) =>
                        {
                            p.phase = GagPhase::Owner {
                                row: None,
                                queued: false,
                            }
                        }
                        result => {
                            let error = format!("gag commit unresolved: {result:?}");
                            p.phase = GagPhase::Write {
                                operation: operation.clone(),
                                uncertain: true,
                            };
                            return Err(error);
                        }
                    }
                }
            }
            GagPhase::Owner { row, queued } => {
                if !*queued {
                    let action = if row.is_some() {
                        StaffAction::GagCommitted(p.proposal.clone())
                    } else {
                        StaffAction::GagRejected(p.proposal.clone())
                    };
                    if self
                        .simulation
                        .input()
                        .try_submit(Command::Staff(StaffCommand {
                            token: p.proposal.operation,
                            action,
                        }))
                        .is_ok()
                    {
                        *queued = true;
                    }
                }
            }
            GagPhase::Capturing(_) => {}
        }
        Ok(())
    }
    pub(super) fn finish_staff_gag(
        &mut self,
        token: u64,
        result: Result<(), bace_gameplay_api::staff::StaffError>,
    ) -> Result<bool, String> {
        let Some(p) = &mut self.staff.gag else {
            return Ok(true);
        };
        let GagPhase::Owner { row, queued } = &mut p.phase else {
            return Err("gag outcome before receipt".into());
        };
        if token != p.proposal.operation || !*queued {
            return Err("gag receipt correlation".into());
        }
        if row.is_some() && result.is_err()
            || row.is_none() && result != Err(bace_gameplay_api::staff::StaffError::Stale)
        {
            *queued = false;
            self.staff.failure = Some(format!("gag owner receipt rejected: {result:?}"));
            return Ok(false);
        }
        if p.proposal.before_revision.is_some() {
            if let Some(row) = row {
                self.online_saves
                    .finish_critical(std::slice::from_ref(row))?;
            } else {
                self.online_saves
                    .cancel_critical(&[p.proposal.target.character.0])?;
            }
        }
        self.staff.gag = None;
        Ok(true)
    }
}
fn operation(
    epoch: u64,
    p: &StaffGagProposal,
    before: &PlayerSaveV6,
    version: i64,
    lease: bace_persistence::CharacterLease,
) -> Result<StaffGagOperation, String> {
    if before.player.entity.object_id != p.target.character.0
        || before.player.account_id != p.target.account.0
        || p.before_revision
            .is_some_and(|r| r != before.player.entity.mutation_revision)
        || version <= 0
        || version == i64::MAX
    {
        return Err("gag snapshot identity/revision".into());
    }
    let next = crate::staff_gags::freeze_offline_gag(before, p.enabled, p.unix_seconds)
        .map_err(|e| e.to_string())?;
    let mut id = [0; 16];
    id[..8].copy_from_slice(&epoch.to_le_bytes());
    id[8..].copy_from_slice(&p.operation.to_le_bytes());
    Ok(StaffGagOperation {
        operation_id: id,
        issuer_account: p.context.account.0,
        lease,
        enabled: p.enabled,
        unix_seconds: p.unix_seconds,
        snapshot: SaveSnapshot {
            object_id: p.target.character.0,
            mutation_revision: next.player.entity.mutation_revision,
            expected_version: version,
            bytes: next.encode().map_err(|e| e.to_string())?,
        },
    })
}

#[cfg(test)]
mod tests;
