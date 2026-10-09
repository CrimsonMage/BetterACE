//! Retained joined default Shop Buy. Every retry after save admission reuses the
//! same operation bytes and ID, and only an exact receipt releases the owner.
use super::*;
mod output;
use crate::saves::{SaveSubmitError, WriteOutcome};
use bace_gameplay_api::CharacterBinding;
use bace_persistence::{OperationOutcome, SaveSnapshot, VendorStockOperation};
use bace_simulation::{
    PlayerSnapshotOperation, PlayerSnapshotOutcome, PlayerSnapshotRequest, VendorBuyReceipt,
    VendorBuyRequest, VendorBuyReservation,
};

pub(super) struct Begin {
    pub key: SessionKey,
    pub context: ActionContext,
    pub vendor: EntityId,
    pub correlation: u64,
    pub epoch: u64,
    pub fence: VendorSourceFence,
    pub requests: Vec<VendorBuyRequest>,
}

pub(super) struct Pending {
    pub key: SessionKey,
    pub context: ActionContext,
    pub vendor: EntityId,
    pub correlation: u64,
    epoch: u64,
    requests: Vec<VendorBuyRequest>,
    prepared: Option<super::buy_preparation::Prepared>,
    ticket: Option<VendorBuyReservation>,
    operation: Option<VendorStockOperation>,
    committed: Vec<SaveSnapshot>,
    receipt: Option<VendorBuyReceipt>,
    critical: bool,
    cancel_requested: bool,
    failures: u32,
    phase: Phase,
}

enum Phase {
    Cold(Job<Result<super::buy_preparation::Prepared, String>>),
    Reserve,
    AwaitReserve,
    Barrier,
    Capture,
    Capturing(u64),
    Freeze(std::sync::Arc<bace_simulation::PlayerReadSnapshot>, u64),
    Save,
    Saving(Job<Result<OperationOutcome, String>>),
    Retry {
        until: tokio::time::Instant,
        error: String,
    },
    Confirm,
    AwaitConfirm,
    Output,
    Publishing(u64),
    Reject,
    AwaitReject,
    Rejected,
    Blocked(String),
}

impl Pending {
    pub(super) fn failure(&self) -> Option<&str> {
        match &self.phase {
            Phase::Blocked(error) | Phase::Retry { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl VendorRuntime {
    pub(super) fn accept_buy_outcome(
        &mut self,
        outcome: VendorOutcome,
    ) -> Result<(), Box<VendorOutcome>> {
        let Some(p) = self.buy.as_mut() else {
            return Err(Box::new(outcome));
        };
        if p.correlation != outcome.correlation {
            return Err(Box::new(outcome));
        }
        match (&p.phase, outcome.result) {
            (Phase::AwaitReserve, Ok(VendorDecision::BuyReserved(ticket)))
                if ticket.vendor == p.vendor
                    && ticket.inventory.actor == p.context.actor
                    && ticket.operation_id
                        == p.prepared.as_ref().map_or("", |x| x.operation_id.as_str()) =>
            {
                p.ticket = Some(*ticket);
                p.phase = if p.cancel_requested {
                    Phase::Reject
                } else {
                    Phase::Barrier
                };
                Ok(())
            }
            (Phase::AwaitReserve, Err(error)) => {
                let _ = error;
                p.phase = Phase::Rejected;
                Ok(())
            }
            (Phase::AwaitReject, Ok(VendorDecision::BuyRejected)) => {
                p.phase = Phase::Rejected;
                Ok(())
            }
            (Phase::AwaitConfirm, Ok(VendorDecision::BuyConfirmed(ticket)))
                if p.ticket.as_ref().is_some_and(|reserved| {
                    ticket.operation == reserved.inventory.operation
                        && ticket.actor == reserved.inventory.actor
                }) =>
            {
                p.phase = Phase::Output;
                Ok(())
            }
            (Phase::AwaitConfirm | Phase::AwaitReject, Err(error)) => {
                p.phase = Phase::Blocked(format!("vendor Buy owner after commit: {error:?}"));
                Ok(())
            }
            (_, result) => Err(Box::new(VendorOutcome {
                correlation: outcome.correlation,
                result,
            })),
        }
    }

    pub(in crate::game_runtime) fn buy_owns_capture(
        &self,
        outcome: &PlayerSnapshotOutcome,
    ) -> bool {
        self.buy.as_ref().is_some_and(
            |p| matches!(p.phase, Phase::Capturing(token) if token == outcome.correlation),
        )
    }

    pub(in crate::game_runtime) fn accept_buy_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        let Some(p) = self.buy.as_mut() else {
            return Err(outcome);
        };
        if !matches!(p.phase, Phase::Capturing(token) if token == outcome.correlation) {
            return Err(outcome);
        }
        if p.cancel_requested {
            p.phase = Phase::Reject;
            return Ok(());
        }
        match outcome.result {
            Ok(snapshot) => {
                p.phase = Phase::Freeze(snapshot, unix);
                Ok(())
            }
            Err(error) => {
                let _ = error;
                p.phase = Phase::Reject;
                Ok(())
            }
        }
    }
}

impl GameRuntime {
    pub(super) fn begin_vendor_buy(&mut self, begin: Begin) -> Result<(), String> {
        if self.vendors.has_pending()
            || begin.epoch == 0
            || begin.requests.is_empty()
            || begin.requests.len() > 1024
        {
            return Err("vendor Buy admission bounds".into());
        }
        let store = self.bootstrap.store.clone();
        let manifest = self.bootstrap.assets.clone();
        let max = self.limits.message_bytes;
        let vendor = begin.vendor;
        let actor = begin.context.actor;
        let fence = begin.fence;
        let requests = begin.requests.clone();
        let job = Box::pin(async move {
            let state = store
                .load_vendor_state(vendor.0)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("vendor Buy durable source absent")?;
            let durable = bace_storage_codec::ItemSaveV5::decode(&state.source.aggregate.bytes)
                .map_err(|e| e.to_string())?;
            if state.source.aggregate.persisted_version <= 0
                || durable.entity.object_id != vendor.0
                || durable.entity.state.weenie_id != fence.template
                || durable.entity.state.weenie_type != 12
                || state.source.cell >> 16 != u32::from(fence.landblock)
                || fence.epoch == 0
            {
                return Err("vendor Buy durable source fence".into());
            }
            let (plans, source, listing) = super::buy_preparation::plan(
                &state,
                vendor,
                fence.revision,
                fence.hash,
                &requests,
                max,
            )?;
            let count =
                u16::try_from(plans.len()).map_err(|_| "vendor Buy grant identity count")?;
            let ids = store
                .allocate_dynamic_ids(count)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(EntityId)
                .collect::<Vec<_>>();
            if ids.len() != plans.len() {
                return Err("vendor Buy grant allocation mismatch".into());
            }
            tokio::task::spawn_blocking(move || {
                let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
                super::buy_preparation::materialize(
                    state,
                    actor,
                    source,
                    listing,
                    &plans,
                    &ids,
                    &mut assets,
                )
            })
            .await
            .map_err(|e| e.to_string())?
        });
        self.vendors.buy = Some(Pending {
            key: begin.key,
            context: begin.context,
            vendor,
            correlation: begin.correlation,
            epoch: begin.epoch,
            requests: begin.requests,
            prepared: None,
            ticket: None,
            operation: None,
            committed: Vec::new(),
            receipt: None,
            critical: false,
            cancel_requested: false,
            failures: 0,
            phase: Phase::Cold(job),
        });
        Ok(())
    }

    pub(super) fn poll_vendor_buy(&mut self) -> Result<(), String> {
        let Some(mut p) = self.vendors.buy.take() else {
            return Ok(());
        };
        let disconnected = self
            .sessions
            .get(&p.key)
            .is_none_or(|session| session.disconnected || session.terminated);
        if disconnected {
            match p.phase {
                Phase::Cold(_) | Phase::Reserve => return Ok(()),
                Phase::AwaitReserve | Phase::Capturing(_) => p.cancel_requested = true,
                Phase::Barrier | Phase::Capture | Phase::Freeze(_, _) | Phase::Save => {
                    p.phase = Phase::Reject;
                }
                _ => {}
            }
        }
        let result = (|| -> Result<(), String> {
            let phase = std::mem::replace(
                &mut p.phase,
                Phase::Blocked("vendor Buy phase transition".into()),
            );
            p.phase = match phase {
                Phase::Cold(mut job) => match poll_job(&mut job) {
                    None => Phase::Cold(job),
                    Some(Ok(prepared))
                        if prepared.state.source.aggregate.object_id == p.vendor.0 =>
                    {
                        p.prepared = Some(prepared);
                        Phase::Reserve
                    }
                    Some(Ok(_)) => Phase::Rejected,
                    Some(Err(_)) => Phase::Rejected,
                },
                Phase::Reserve => {
                    let prepared = p
                        .prepared
                        .as_ref()
                        .ok_or("vendor Buy prepared source missing")?;
                    let command = Command::Vendor(Box::new(VendorCommand {
                        correlation: p.correlation,
                        action: VendorAction::ReserveDefaultBuy {
                            context: p.context,
                            vendor: p.vendor,
                            source: prepared.source,
                            requests: p.requests.clone(),
                            prepared: prepared.inventory.clone(),
                            operation_id: prepared.operation_id.clone(),
                        },
                    }));
                    match self.simulation.input().try_submit(command) {
                        Ok(()) => Phase::AwaitReserve,
                        Err(TrySendError::Full(_)) => Phase::Reserve,
                        Err(TrySendError::Disconnected(_)) => {
                            Phase::Blocked("vendor Buy simulation disconnected".into())
                        }
                    }
                }
                Phase::Barrier => {
                    if self.online_saves.critical_ready(&[p.context.actor.0])? {
                        self.online_saves.begin_critical(&[p.context.actor.0])?;
                        p.critical = true;
                        Phase::Capture
                    } else {
                        Phase::Barrier
                    }
                }
                Phase::Capture => {
                    let ticket = p.ticket.as_ref().ok_or("vendor Buy reservation missing")?;
                    let token = self.token()?;
                    let command = Command::PlayerSnapshot(PlayerSnapshotRequest {
                        correlation: token,
                        binding: CharacterBinding {
                            actor: p.context.actor,
                            account: p.context.account,
                            session: p.context.session,
                        },
                        operation: Some((
                            PlayerSnapshotOperation::VendorBuy(ticket.inventory.operation),
                            ticket.actor_revision,
                        )),
                    });
                    match self.simulation.input().try_submit(command) {
                        Ok(()) => Phase::Capturing(token),
                        Err(TrySendError::Full(_)) => Phase::Capture,
                        Err(TrySendError::Disconnected(_)) => {
                            Phase::Blocked("vendor Buy simulation disconnected".into())
                        }
                    }
                }
                Phase::Freeze(snapshot, unix) => {
                    let ticket = p.ticket.as_ref().ok_or("vendor Buy ticket missing")?;
                    let prepared = p
                        .prepared
                        .as_ref()
                        .ok_or("vendor Buy prepared state missing")?;
                    match super::buy_freezing::freeze_captured(
                        ticket,
                        &snapshot,
                        &self.online_saves,
                        &prepared.fresh,
                        &prepared.state,
                        p.epoch,
                        unix,
                    ) {
                        Ok(operation) => {
                            // The saved bytes are the exact proposed after state.
                            // Preflight their full private transcript before SQL:
                            // NetworkThread's reliable admission is bounded to 256
                            // messages and one configured message byte budget.
                            p.committed = operation.inventory.snapshots.clone();
                            p.operation = Some(operation);
                            if self.preflight_vendor_buy_output(&p).is_err() {
                                p.committed.clear();
                                p.operation = None;
                                self.online_saves.cancel_critical(&[p.context.actor.0])?;
                                p.critical = false;
                                Phase::Reject
                            } else {
                                p.committed.clear();
                                Phase::Save
                            }
                        }
                        Err(_) => {
                            self.online_saves.cancel_critical(&[p.context.actor.0])?;
                            p.critical = false;
                            Phase::Reject
                        }
                    }
                }
                Phase::Save => {
                    let operation = p.operation.as_ref().ok_or("vendor Buy operation missing")?;
                    match save_future(operation, &self.saves.handle) {
                        Ok(job) => Phase::Saving(job),
                        Err(SaveSubmitError::Full) => Phase::Save,
                        Err(error) => Phase::Blocked(format!("vendor Buy save admission: {error}")),
                    }
                }
                Phase::Saving(mut job) => match poll_job(&mut job) {
                    None => Phase::Saving(job),
                    Some(Ok(outcome)) => {
                        let (receipt, committed) = super::buy_freezing::receipt(
                            p.ticket.as_ref().ok_or("vendor Buy ticket missing")?,
                            p.operation.as_ref().ok_or("vendor Buy operation missing")?,
                            outcome,
                        )?;
                        p.receipt = Some(receipt);
                        p.committed = committed;
                        Phase::Confirm
                    }
                    Some(Err(error)) => {
                        p.failures = p.failures.saturating_add(1);
                        Phase::Retry {
                            until: tokio::time::Instant::now()
                                + Duration::from_secs(1_u64 << p.failures.min(5)),
                            error,
                        }
                    }
                },
                Phase::Retry { until, error } => {
                    if tokio::time::Instant::now() < until {
                        Phase::Retry { until, error }
                    } else {
                        match save_future(
                            p.operation.as_ref().ok_or("vendor Buy operation missing")?,
                            &self.saves.handle,
                        ) {
                            Ok(job) => Phase::Saving(job),
                            Err(SaveSubmitError::Full) => Phase::Retry { until, error },
                            Err(failure) => Phase::Blocked(format!(
                                "vendor Buy save retry admission: {failure}"
                            )),
                        }
                    }
                }
                Phase::Confirm => {
                    let command = Command::Vendor(Box::new(VendorCommand {
                        correlation: p.correlation,
                        action: VendorAction::ConfirmBuy {
                            ticket: Box::new(
                                p.ticket
                                    .as_ref()
                                    .ok_or("vendor Buy ticket missing")?
                                    .clone(),
                            ),
                            receipt: p
                                .receipt
                                .as_ref()
                                .ok_or("vendor Buy receipt missing")?
                                .clone(),
                        },
                    }));
                    match self.simulation.input().try_submit(command) {
                        Ok(()) => Phase::AwaitConfirm,
                        Err(TrySendError::Full(_)) => Phase::Confirm,
                        Err(TrySendError::Disconnected(_)) => {
                            Phase::Blocked("vendor Buy simulation disconnected after commit".into())
                        }
                    }
                }
                Phase::Reject => {
                    if p.critical {
                        self.online_saves.cancel_critical(&[p.context.actor.0])?;
                        p.critical = false;
                    }
                    let command = Command::Vendor(Box::new(VendorCommand {
                        correlation: p.correlation,
                        action: VendorAction::RejectBuy(Box::new(
                            p.ticket
                                .as_ref()
                                .ok_or("vendor Buy ticket missing")?
                                .clone(),
                        )),
                    }));
                    match self.simulation.input().try_submit(command) {
                        Ok(()) => Phase::AwaitReject,
                        Err(TrySendError::Full(_)) => Phase::Reject,
                        Err(TrySendError::Disconnected(_)) => Phase::Blocked(
                            "vendor Buy simulation disconnected during rejection".into(),
                        ),
                    }
                }
                other => other,
            };
            Ok(())
        })();
        if let Err(error) = &result {
            p.phase = Phase::Blocked(format!("vendor Buy retained transition: {error}"));
        }
        self.vendors.buy = Some(p);
        result
    }
}

fn save_future(
    operation: &VendorStockOperation,
    saves: &crate::saves::SaveHandle,
) -> Result<Job<Result<OperationOutcome, String>>, SaveSubmitError> {
    let ticket = saves.try_vendor_stock(operation)?;
    Ok(Box::pin(async move {
        let report = ticket
            .await
            .map_err(|_| "vendor Buy save reply closed".to_string())?;
        match report.result {
            Ok(WriteOutcome::Valuable(outcome)) => Ok(outcome),
            Ok(_) => Err("vendor Buy save receipt kind".into()),
            Err(error) => Err(error.to_string()),
        }
    }))
}
