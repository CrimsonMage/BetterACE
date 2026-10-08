//! Retained first-Use Shop owner. Buy/Sell stay closed until their complete
//! player currency and stock transaction has an exact durable receipt.
mod preparation;
#[cfg(test)]
mod tests;
use super::*;
use bace_gameplay_api::ActionContext;
use bace_persistence::{OperationOutcome, VendorStockOperation};
use bace_simulation::{
    Command, VendorAction, VendorCommand, VendorDecision, VendorLazyStockReceipt,
    VendorLazyStockTicket, VendorOutcome,
};
use bace_transport::ReceivedMessage;
use bace_types::EntityId;
use bace_wire::VendorListing;
use bace_wire::{GameActionEnvelope, InventoryAction};
use std::sync::mpsc::TrySendError;

pub(super) struct VendorRuntime {
    pending: Option<Pending>,
    completion: Option<VendorCompletion>,
    unexpected: Option<VendorOutcome>,
    publication: Option<(u64, SessionKey, Vec<u8>)>,
    next_publication: u64,
}

pub(super) struct VendorCompletion {
    pub key: SessionKey,
    pub context: ActionContext,
    pub ticket: VendorLazyStockTicket,
    pub listing: VendorListing,
}

#[derive(Clone, Copy)]
struct VendorSourceFence {
    template: u32,
    revision: u64,
    hash: [u8; 32],
    landblock: u16,
    epoch: u64,
}

struct Begin {
    key: SessionKey,
    context: ActionContext,
    vendor: EntityId,
    correlation: u64,
    epoch: u64,
    fence: VendorSourceFence,
    max_message_bytes: usize,
    store: bace_db_postgres::PgStore,
    generation: Arc<bace_storage_codec::PackGeneration>,
}

struct Pending {
    key: SessionKey,
    context: ActionContext,
    vendor: EntityId,
    correlation: u64,
    epoch: u64,
    prepared: Option<preparation::Prepared>,
    restored: Option<preparation::Restored>,
    ticket: Option<VendorLazyStockTicket>,
    operation: Option<VendorStockOperation>,
    listing: Option<VendorListing>,
    save_failures: u32,
    phase: Phase,
}

enum Phase {
    Cold(Job<Result<ColdPrepared, String>>),
    SubmitReserve,
    AwaitReserve,
    SubmitAdopt,
    AwaitAdopt,
    ReadySave,
    Saving(Job<Result<OperationOutcome, String>>),
    RetrySave {
        until: tokio::time::Instant,
        error: String,
    },
    SubmitConfirm(VendorLazyStockReceipt),
    AwaitConfirm,
    Rejected(String),
    Blocked(String),
}

enum ColdPrepared {
    Fresh(preparation::Prepared),
    Restored(preparation::Restored),
}

impl VendorRuntime {
    pub(super) fn new() -> Self {
        Self {
            pending: None,
            completion: None,
            unexpected: None,
            publication: None,
            next_publication: 0,
        }
    }

    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
            || self.completion.is_some()
            || self.unexpected.is_some()
            || self.publication.is_some()
    }

    pub(super) fn ingress_blocked(&self, key: SessionKey) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.key == key)
            || self
                .completion
                .as_ref()
                .is_some_and(|completion| completion.key == key)
            || self
                .publication
                .as_ref()
                .is_some_and(|(_, owner, _)| *owner == key)
    }

    pub(super) fn failure(&self) -> Option<&str> {
        match self.pending.as_ref().map(|pending| &pending.phase) {
            Some(Phase::Blocked(error) | Phase::RetrySave { error, .. }) => Some(error),
            _ => None,
        }
    }

    fn begin(
        &mut self,
        Begin {
            key,
            context,
            vendor,
            correlation,
            epoch,
            fence,
            max_message_bytes,
            store,
            generation,
        }: Begin,
    ) -> Result<(), String> {
        if self.has_pending()
            || vendor.0 == 0
            || correlation == 0
            || epoch == 0
            || fence.revision == 0
            || fence.hash == [0; 32]
            || fence.epoch == 0
        {
            return Err("vendor first-Use owner busy or invalid".into());
        }
        let phase = Phase::Cold(Box::pin(async move {
            let state = store
                .load_vendor_state(vendor.0)
                .await
                .map_err(|error| error.to_string())?
                .ok_or("vendor has no durable world source")?;
            let stored = state.source;
            if stored.aggregate.persisted_version <= 0 {
                return Err("vendor source is not durably committed".into());
            }
            let forest = state.forest;
            let saved = bace_storage_codec::ItemSaveV5::decode(&stored.aggregate.bytes)
                .map_err(|error| error.to_string())?;
            if saved.entity.object_id != vendor.0
                || saved.entity.state.weenie_type != 12
                || saved.entity.state.weenie_id != fence.template
                || saved.entity.template_revision != fence.revision
                || stored.cell >> 16 != u32::from(fence.landblock)
            {
                return Err("durable vendor source subtype".into());
            }
            let source_hash = fence.hash;
            let source_revision = fence.revision;
            let source = saved.entity.state.clone();
            if let Some(forest) = forest {
                return tokio::task::spawn_blocking(move || {
                    preparation::restore(preparation::RestoreInput {
                        vendor,
                        vendor_expected_version: stored.aggregate.persisted_version,
                        source,
                        source_revision,
                        source_hash,
                        max_message_bytes,
                        forest,
                    })
                })
                .await
                .map_err(|error| error.to_string())?
                .map(ColdPrepared::Restored);
            }
            let templates = tokio::task::spawn_blocking({
                let generation = generation.clone();
                let source = source.clone();
                move || preparation::load_templates(&generation, &source)
            })
            .await
            .map_err(|error| error.to_string())??;
            let count = preparation::required_ids(&source, &templates, max_message_bytes)?;
            let count = u16::try_from(count).map_err(|_| "vendor identity count")?;
            let ids = store
                .allocate_dynamic_ids(count)
                .await
                .map_err(|error| error.to_string())?;
            let ids: Vec<_> = ids.into_iter().map(EntityId).collect();
            if ids.len() != usize::from(count) {
                return Err("vendor identity allocation count".into());
            }
            tokio::task::spawn_blocking(move || {
                preparation::prepare(preparation::Input {
                    vendor,
                    vendor_expected_version: stored.aggregate.persisted_version,
                    source,
                    source_revision,
                    source_hash,
                    world_epoch: epoch,
                    max_message_bytes,
                    operation_id: format!("vendor-load-{}-{}", vendor.0, ids[0].0),
                    ids,
                    templates,
                })
            })
            .await
            .map_err(|error| error.to_string())?
            .map(ColdPrepared::Fresh)
        }));
        self.pending = Some(Pending {
            key,
            context,
            vendor,
            correlation,
            epoch,
            prepared: None,
            restored: None,
            ticket: None,
            operation: None,
            listing: None,
            save_failures: 0,
            phase,
        });
        Ok(())
    }

    pub(super) fn accept_outcome(
        &mut self,
        outcome: VendorOutcome,
    ) -> Result<(), Box<VendorOutcome>> {
        let Some(pending) = self.pending.as_mut() else {
            return Err(Box::new(outcome));
        };
        if pending.correlation != outcome.correlation {
            return Err(Box::new(outcome));
        }
        match (&pending.phase, outcome.result) {
            (Phase::AwaitReserve, Ok(VendorDecision::Reserved(ticket)))
                if ticket.vendor == pending.vendor && pending.prepared.is_some() =>
            {
                let prepared = pending.prepared.take().expect("matched prepared vendor");
                pending.listing = Some(prepared.listing.clone());
                let operation = match prepared.freeze(&ticket, pending.epoch) {
                    Ok(operation) => operation,
                    Err(error) => {
                        pending.phase = Phase::Blocked(error);
                        return Ok(());
                    }
                };
                pending.ticket = Some(ticket);
                pending.operation = Some(operation);
                pending.phase = Phase::ReadySave;
                Ok(())
            }
            (Phase::AwaitConfirm, Ok(VendorDecision::Confirmed)) => {
                let finished = self.pending.take().expect("matched vendor confirmation");
                self.completion = Some(VendorCompletion {
                    key: finished.key,
                    context: finished.context,
                    ticket: finished.ticket.expect("confirmed vendor ticket"),
                    listing: finished.listing.expect("confirmed vendor listing"),
                });
                Ok(())
            }
            (Phase::AwaitAdopt, Ok(VendorDecision::Adopted(ticket)))
                if ticket.vendor == pending.vendor =>
            {
                let finished = self.pending.take().expect("matched vendor adoption");
                self.completion = Some(VendorCompletion {
                    key: finished.key,
                    context: finished.context,
                    ticket,
                    listing: finished.listing.expect("adopted vendor listing"),
                });
                Ok(())
            }
            (Phase::AwaitReserve, Err(error)) => {
                pending.phase = Phase::Rejected(format!("vendor Use rejected: {error:?}"));
                Ok(())
            }
            (Phase::AwaitAdopt | Phase::AwaitConfirm, Err(error)) => {
                pending.phase = Phase::Blocked(format!("vendor simulation owner: {error:?}"));
                Ok(())
            }
            (_, result) => Err(Box::new(VendorOutcome {
                correlation: outcome.correlation,
                result,
            })),
        }
    }

    pub(super) fn poll(
        &mut self,
        input: &crate::simulation::SimulationInput,
        store: &bace_db_postgres::PgStore,
    ) -> Result<(), String> {
        if self.unexpected.is_some() {
            return Err("unmatched vendor outcome retained".into());
        }
        let Some(pending) = self.pending.as_mut() else {
            return Ok(());
        };
        let phase = std::mem::replace(
            &mut pending.phase,
            Phase::Blocked("vendor poll transition".into()),
        );
        pending.phase = match phase {
            Phase::Cold(mut job) => match poll_job(&mut job) {
                None => Phase::Cold(job),
                Some(Ok(ColdPrepared::Fresh(prepared)))
                    if prepared.batch.vendor == pending.vendor =>
                {
                    pending.prepared = Some(prepared);
                    Phase::SubmitReserve
                }
                Some(Ok(ColdPrepared::Restored(restored)))
                    if restored.batch.vendor == pending.vendor =>
                {
                    pending.listing = Some(restored.listing.clone());
                    pending.restored = Some(restored);
                    Phase::SubmitAdopt
                }
                Some(Ok(_)) => Phase::Rejected("vendor cold identity mismatch".into()),
                Some(Err(error)) => Phase::Rejected(error),
            },
            Phase::SubmitReserve => {
                let batch = pending
                    .prepared
                    .as_ref()
                    .ok_or("vendor prepared work missing")?
                    .batch
                    .clone();
                let command = Command::Vendor(Box::new(VendorCommand {
                    correlation: pending.correlation,
                    action: VendorAction::Reserve {
                        context: pending.context,
                        batch: Box::new(batch),
                    },
                }));
                match input.try_submit(command) {
                    Ok(()) => Phase::AwaitReserve,
                    Err(TrySendError::Full(_)) => Phase::SubmitReserve,
                    Err(TrySendError::Disconnected(_)) => {
                        Phase::Blocked("vendor simulation disconnected".into())
                    }
                }
            }
            Phase::SubmitAdopt => {
                let restored = pending
                    .restored
                    .as_ref()
                    .ok_or("vendor restored work missing")?;
                let command = Command::Vendor(Box::new(VendorCommand {
                    correlation: pending.correlation,
                    action: VendorAction::Adopt {
                        context: pending.context,
                        batch: Box::new(restored.batch.clone()),
                        receipt: restored.receipt.clone(),
                    },
                }));
                match input.try_submit(command) {
                    Ok(()) => Phase::AwaitAdopt,
                    Err(TrySendError::Full(_)) => Phase::SubmitAdopt,
                    Err(TrySendError::Disconnected(_)) => {
                        Phase::Blocked("vendor simulation disconnected".into())
                    }
                }
            }
            Phase::ReadySave => Phase::Saving(save_job_future(
                pending
                    .operation
                    .as_ref()
                    .ok_or("vendor operation missing")?,
                store.clone(),
            )),
            Phase::Saving(mut job) => match poll_job(&mut job) {
                None => Phase::Saving(job),
                Some(Ok(result)) => {
                    let ticket = pending.ticket.as_ref().ok_or("vendor ticket missing")?;
                    let receipt = receipt(ticket, result)?;
                    Phase::SubmitConfirm(receipt)
                }
                Some(Err(error)) => {
                    // The same operation ID, snapshots and marker are retried;
                    // SQL replay validates the fingerprint after uncertainty.
                    pending.save_failures = pending.save_failures.saturating_add(1);
                    let seconds = 1_u64 << pending.save_failures.min(5);
                    Phase::RetrySave {
                        until: tokio::time::Instant::now() + Duration::from_secs(seconds),
                        error,
                    }
                }
            },
            Phase::RetrySave { until, error } => {
                if tokio::time::Instant::now() >= until {
                    Phase::Saving(save_job_future(
                        pending
                            .operation
                            .as_ref()
                            .ok_or("vendor operation missing")?,
                        store.clone(),
                    ))
                } else {
                    Phase::RetrySave { until, error }
                }
            }
            Phase::SubmitConfirm(receipt) => {
                let command = Command::Vendor(Box::new(VendorCommand {
                    correlation: pending.correlation,
                    action: VendorAction::Confirm(receipt.clone()),
                }));
                match input.try_submit(command) {
                    Ok(()) => Phase::AwaitConfirm,
                    Err(TrySendError::Full(_)) => Phase::SubmitConfirm(receipt),
                    Err(TrySendError::Disconnected(_)) => {
                        Phase::Blocked("vendor simulation disconnected after commit".into())
                    }
                }
            }
            other => other,
        };
        Ok(())
    }
}

impl GameRuntime {
    pub(in crate::game_runtime) fn vendor_ingress_blocked(&self, key: SessionKey) -> bool {
        self.vendors.ingress_blocked(key)
    }
    pub(in crate::game_runtime) fn handle_vendor_message(
        &mut self,
        key: SessionKey,
        message: &ReceivedMessage,
    ) -> Result<inventory::InventoryIngress, String> {
        use inventory::InventoryIngress as I;
        let Some(loaded) = self
            .sessions
            .get(&key)
            .and_then(|session| session.loading.as_ref())
        else {
            return Ok(I::Unsupported);
        };
        let binding = loaded.loaded.binding;
        let envelope = match GameActionEnvelope::decode(&message.bytes, self.limits.message_bytes) {
            Ok(envelope) => envelope,
            Err(_) => return Ok(I::Unsupported),
        };
        if !matches!(
            envelope.action,
            bace_wire::opcode::GameActionType::Use
                | bace_wire::opcode::GameActionType::Buy
                | bace_wire::opcode::GameActionType::Sell
        ) {
            return Ok(I::Unsupported);
        }
        let request = bace_wire::InventoryRequest::decode(
            envelope.action,
            envelope.payload,
            self.limits.message_bytes,
            1024,
        )
        .map_err(|error| format!("vendor request: {error:?}"))?;
        let vendor = match request.action {
            InventoryAction::Use(id)
            | InventoryAction::Buy { vendor_id: id, .. }
            | InventoryAction::Sell { vendor_id: id, .. } => EntityId(id),
            _ => return Ok(I::Unsupported),
        };
        let Some(registration) = self.npc.vendor_registration(vendor) else {
            return Ok(I::Unsupported);
        };
        let fence = VendorSourceFence {
            template: registration.source.template,
            revision: registration.generation.revision(),
            hash: registration.source.program_hash,
            landblock: registration.landblock,
            epoch: registration.epoch,
        };
        let generation = registration.generation.clone();
        if !matches!(request.action, InventoryAction::Use(_)) {
            // Commerce requires the stock marker, player currency and item
            // forest to share an exact receipt; no partial success is emitted.
            return Ok(I::Unsupported);
        }
        if self.vendors.has_pending() {
            return Ok(I::Blocked);
        }
        if self
            .bootstrap
            .save_pressure
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Ok(I::Blocked);
        }
        if !self.players.entered(binding.actor)
            || self
                .sessions
                .get(&key)
                .is_some_and(|session| session.terminated || session.disconnected)
            || binding.session.0 != key.generation
            || binding.account
                != self
                    .sessions
                    .get(&key)
                    .ok_or("vendor session missing")?
                    .account
                    .id
        {
            return Ok(I::Blocked);
        }
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: envelope.sequence,
        };
        let correlation = self.token()?;
        self.vendors.begin(Begin {
            key,
            context,
            vendor,
            correlation,
            epoch: self.bootstrap.world_owner.epoch(),
            fence,
            max_message_bytes: self.limits.message_bytes,
            store: self.bootstrap.store.clone(),
            generation,
        })?;
        Ok(I::Accepted)
    }

    pub(super) fn poll_vendors(&mut self) -> Result<(), String> {
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.vendor_outcomes().try_recv() else {
                break;
            };
            if let Err(unexpected) = self.vendors.accept_outcome(outcome) {
                self.vendors.unexpected = Some(*unexpected);
                return Err("unmatched vendor outcome retained".into());
            }
        }
        self.vendors
            .poll(&self.simulation.input(), &self.bootstrap.store)?;
        if let Some(error) = self.vendors.failure() {
            return Err(format!("vendor retained: {error}"));
        }
        self.project_vendor_output()
    }

    fn project_vendor_output(&mut self) -> Result<(), String> {
        const PREFIX: u64 = 0x5600_0000_0000_0000;
        const MASK: u64 = 0xff00_0000_0000_0000;
        if self
            .vendors
            .pending
            .as_ref()
            .is_some_and(|pending| matches!(pending.phase, Phase::Rejected(_)))
        {
            if self.network_output.len() >= self.limits.messages {
                return Ok(());
            }
            let rejected = self
                .vendors
                .pending
                .take()
                .expect("matched rejected vendor Use");
            if let Phase::Rejected(reason) = rejected.phase
                && let Some(session) = self.sessions.get_mut(&rejected.key)
            {
                session.failure = Some(reason);
            }
            self.network_output
                .push_back(NetworkCommand::Terminate { key: rejected.key });
            return Ok(());
        }
        if let Some(index) = self
            .reliable_admissions
            .iter()
            .position(|(_, id, _)| id & MASK == PREFIX)
        {
            let (key, correlation, accepted) = self.reliable_admissions[index];
            if self
                .vendors
                .publication
                .as_ref()
                .is_none_or(|(id, owner, _)| *id != correlation || *owner != key)
            {
                return Err("vendor reliable publication receipt mismatch".into());
            }
            self.reliable_admissions.remove(index);
            self.vendors.publication = None;
            self.vendors.completion = None;
            if !accepted && let Some(session) = self.sessions.get_mut(&key) {
                session.terminated = true;
            }
        }
        let Some(completion) = self.vendors.completion.as_ref() else {
            return Ok(());
        };
        if completion.listing.vendor_id != completion.ticket.vendor.0 {
            return Err("vendor confirmed listing identity mismatch".into());
        }
        if self.vendors.publication.is_some() {
            return Ok(());
        }
        if self
            .sessions
            .get(&completion.key)
            .is_none_or(|session| session.disconnected || session.terminated)
        {
            self.vendors.completion = None;
            return Ok(());
        }
        if self.network_output.len() >= self.limits.messages
            || self.vendors.next_publication == !MASK
        {
            return Ok(());
        }
        let bytes = completion
            .listing
            .encode(
                completion.context.actor.0,
                completion.context.sequence,
                1024,
                preparation::listing_limits(self.limits.message_bytes),
            )
            .map_err(|error| format!("vendor ApproachVendor output: {error:?}"))?;
        self.vendors.next_publication += 1;
        let correlation = PREFIX | self.vendors.next_publication;
        let key = completion.key;
        self.vendors.publication = Some((correlation, key, bytes.clone()));
        self.network_output
            .push_back(NetworkCommand::SendReliableBatch {
                key,
                correlation,
                messages: vec![(9, bytes)],
            });
        Ok(())
    }
}

fn poll_job<T>(job: &mut Job<T>) -> Option<T> {
    match job.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

fn save_job_future(
    operation: &VendorStockOperation,
    store: bace_db_postgres::PgStore,
) -> Job<Result<OperationOutcome, String>> {
    let operation = operation.clone();
    Box::pin(async move {
        store
            .vendor_stock_operation(&operation)
            .await
            .map_err(|error| error.to_string())
    })
}

fn receipt(
    ticket: &VendorLazyStockTicket,
    result: OperationOutcome,
) -> Result<VendorLazyStockReceipt, String> {
    let items = match result {
        OperationOutcome::Committed(acks) => {
            let by_id: BTreeMap<_, _> = acks
                .into_iter()
                .map(|ack| (ack.object_id, ack.persisted_version))
                .collect();
            if by_id.len() != ticket.item_ids.len() + 1
                || by_id.get(&ticket.marker.0) != Some(&1)
                || ticket
                    .item_ids
                    .iter()
                    .any(|id| by_id.get(&id.0) != Some(&1))
            {
                return Err("vendor stock durable acknowledgement mismatch".into());
            }
            ticket.item_ids.iter().map(|id| (*id, 1)).collect()
        }
        OperationOutcome::AlreadyCommitted => ticket.item_ids.iter().map(|id| (*id, 1)).collect(),
    };
    Ok(VendorLazyStockReceipt {
        vendor: ticket.vendor,
        marker: ticket.marker,
        operation_id: ticket.operation_id.clone(),
        marker_version: 1,
        items,
    })
}
