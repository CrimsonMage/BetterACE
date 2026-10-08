//! Bounded write service with reserved routine capacity and bounded critical bursts.
//! Simulation owns coalescing/retry state; this worker never clears dirty state on failure.
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::{
    CharacterLease, OfflineXpEvent, OperationOutcome, SAVE_INTERVAL, SaveAck, SaveSnapshot,
    XpReceipt,
};
use bace_persistence::{
    HousingOperation, InventoryOperation, NpcStageOperation, OwnedSaveBatch, OwnershipState,
    PlacementOperation, WorldPlacementOperation,
};
use bace_storage_codec::HouseSaveV1;
mod gameplay;
use gameplay::GameplayWrite;
pub use gameplay::HousingWrite;
use std::{collections::VecDeque, future::Future, sync::Arc, time::Duration};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot, watch},
    task::JoinHandle,
    time::Instant,
};

#[derive(Debug, Clone)]
pub struct SaveWorkerConfig {
    pub offline_capacity: usize,
    pub offline_bytes: u32,
    pub routine_capacity: usize,
    pub valuable_capacity: usize,
    pub routine_bytes: u32,
    pub valuable_bytes: u32,
    pub maximum_valuable_burst: usize,
    pub admission_per_turn: usize,
    pub operation_timeout: Duration,
}
impl Default for SaveWorkerConfig {
    fn default() -> Self {
        Self {
            offline_capacity: 128,
            offline_bytes: 1024 * 1024,
            routine_capacity: 128,
            valuable_capacity: 32,
            routine_bytes: 64 * 1024 * 1024,
            valuable_bytes: 16 * 1024 * 1024,
            maximum_valuable_burst: 4,
            admission_per_turn: 8,
            operation_timeout: Duration::from_secs(5),
        }
    }
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SaveSubmitError {
    #[error("invalid save worker configuration or request")]
    Invalid,
    #[error("reserved save queue/byte budget is full; caller retains ownership")]
    Full,
    #[error("save worker is closed; caller retains ownership")]
    Closed,
}
#[derive(Debug, thiserror::Error)]
pub enum SaveFailure {
    #[error("storage operation timed out; commit outcome is uncertain and must be resolved")]
    Timeout,
    #[error("storage operation failed (uncertain commit={uncertain}): {message}")]
    Storage { message: String, uncertain: bool },
}
#[derive(Debug)]
pub enum WriteOutcome {
    Offline(XpReceipt),
    Routine(Vec<SaveAck>),
    Valuable(OperationOutcome),
    Allegiance(bace_persistence::AllegianceCommit),
}
#[derive(Debug)]
pub struct SaveReport {
    pub result: Result<WriteOutcome, SaveFailure>,
    /// On failure, return this unchanged to the dirty owner for retry/reconciliation.
    pub dirty_since: Option<Instant>,
    pub started_late_by: Duration,
}
pub type SaveTicket = oneshot::Receiver<SaveReport>;
#[derive(Debug, Default)]
pub struct SaveWorkerSummary {
    pub offline_completed: u64,
    pub routine_completed: u64,
    pub valuable_completed: u64,
    pub failed: u64,
    pub shutdown_timed_out: bool,
}

/// Testable adapter contract. Real production construction uses a dedicated PgStore pool.
pub trait SaveBackend: Send + Sync + 'static {
    fn allegiance_placement(
        &self,
        _operation: &bace_persistence::AllegiancePlacementOperation,
    ) -> impl Future<Output = Result<OperationOutcome, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "allegiance placement backend unsupported".into(),
                uncertain: false,
            })
        }
    }
    fn allegiance(
        &self,
        _operation: &bace_persistence::AllegianceOperation,
    ) -> impl Future<Output = Result<bace_persistence::AllegianceCommit, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "allegiance backend unsupported".into(),
                uncertain: false,
            })
        }
    }
    fn world_placement(
        &self,
        _operation: &WorldPlacementOperation,
    ) -> impl Future<Output = Result<OperationOutcome, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "world placement backend unsupported".into(),
                uncertain: false,
            })
        }
    }
    fn vendor_stock(
        &self,
        _operation: &bace_persistence::VendorStockOperation,
    ) -> impl Future<Output = Result<OperationOutcome, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "vendor stock backend unsupported".into(),
                uncertain: false,
            })
        }
    }
    fn constructed_creature_promotion(
        &self,
        _operation: &bace_persistence::ConstructedCreaturePromotionOperation,
    ) -> impl Future<Output = Result<OperationOutcome, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "constructed creature promotion backend unsupported".into(),
                uncertain: false,
            })
        }
    }

    fn npc_stage(
        &self,
        _operation: &NpcStageOperation,
    ) -> impl Future<Output = Result<OperationOutcome, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "NPC stage backend unsupported".into(),
                uncertain: false,
            })
        }
    }

    fn placement(
        &self,
        _operation: &PlacementOperation,
    ) -> impl Future<Output = Result<OperationOutcome, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "placement backend unsupported".into(),
                uncertain: false,
            })
        }
    }
    fn housing_operation(
        &self,
        _operation: &HousingOperation,
    ) -> impl Future<Output = Result<OperationOutcome, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "housing lifecycle backend unsupported".into(),
                uncertain: false,
            })
        }
    }
    fn inventory(
        &self,
        _operation: &InventoryOperation,
    ) -> impl Future<Output = Result<OperationOutcome, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "inventory backend unsupported".into(),
                uncertain: false,
            })
        }
    }
    fn housing(
        &self,
        _operation: &HousingWrite,
    ) -> impl Future<Output = Result<OperationOutcome, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "housing backend unsupported".into(),
                uncertain: false,
            })
        }
    }
    fn routine(
        &self,
        snapshots: &[SaveSnapshot],
    ) -> impl Future<Output = Result<Vec<SaveAck>, SaveFailure>> + Send;
    fn valuable(
        &self,
        operation_id: &str,
        snapshots: &[SaveSnapshot],
    ) -> impl Future<Output = Result<OperationOutcome, SaveFailure>> + Send;
    fn offline(
        &self,
        _event: &OfflineXpEvent,
        _lease: CharacterLease,
    ) -> impl Future<Output = Result<XpReceipt, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "offline backend unsupported".into(),
                uncertain: false,
            })
        }
    }
    fn owned_batch(
        &self,
        _batch: &OwnedSaveBatch,
    ) -> impl Future<Output = Result<Vec<SaveAck>, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "owned batch backend unsupported".into(),
                uncertain: false,
            })
        }
    }
    fn owned_routine(
        &self,
        _lease: CharacterLease,
        _snapshot: &SaveSnapshot,
    ) -> impl Future<Output = Result<SaveAck, SaveFailure>> + Send {
        async {
            Err(SaveFailure::Storage {
                message: "owned backend unsupported".into(),
                uncertain: false,
            })
        }
    }
    fn close(&self) -> impl Future<Output = ()> + Send {
        async {}
    }
}
impl SaveBackend for PgStore {
    async fn allegiance_placement(
        &self,
        operation: &bace_persistence::AllegiancePlacementOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        self.allegiance_placement_operation(operation)
            .await
            .map_err(storage_failure)
    }
    async fn allegiance(
        &self,
        operation: &bace_persistence::AllegianceOperation,
    ) -> Result<bace_persistence::AllegianceCommit, SaveFailure> {
        self.allegiance_operation(operation)
            .await
            .map_err(storage_failure)
    }
    async fn world_placement(
        &self,
        operation: &WorldPlacementOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        PgStore::world_placement_operation(self, operation)
            .await
            .map_err(storage_failure)
    }
    async fn vendor_stock(
        &self,
        operation: &bace_persistence::VendorStockOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        PgStore::vendor_stock_operation(self, operation)
            .await
            .map_err(storage_failure)
    }
    async fn constructed_creature_promotion(
        &self,
        operation: &bace_persistence::ConstructedCreaturePromotionOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        PgStore::constructed_creature_promotion(self, operation)
            .await
            .map_err(storage_failure)
    }

    async fn npc_stage(
        &self,
        operation: &NpcStageOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        PgStore::npc_stage(self, operation)
            .await
            .map_err(storage_failure)
    }

    async fn placement(
        &self,
        operation: &PlacementOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        self.placement_operation(operation)
            .await
            .map_err(storage_failure)
    }
    async fn housing_operation(
        &self,
        operation: &HousingOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        PgStore::housing_operation(self, operation)
            .await
            .map_err(storage_failure)
    }

    async fn inventory(
        &self,
        operation: &InventoryOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        self.inventory_operation(operation)
            .await
            .map_err(storage_failure)
    }
    async fn housing(&self, operation: &HousingWrite) -> Result<OperationOutcome, SaveFailure> {
        self.save_house(
            &operation.operation_id,
            operation.owner,
            &operation.house,
            operation.expected_version,
        )
        .await
        .map_err(storage_failure)
    }
    async fn routine(&self, snapshots: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        self.save_batch(snapshots).await.map_err(storage_failure)
    }
    async fn valuable(
        &self,
        operation_id: &str,
        snapshots: &[SaveSnapshot],
    ) -> Result<OperationOutcome, SaveFailure> {
        PgStore::valuable(self, operation_id, snapshots)
            .await
            .map_err(storage_failure)
    }
    async fn offline(
        &self,
        event: &OfflineXpEvent,
        lease: CharacterLease,
    ) -> Result<XpReceipt, SaveFailure> {
        self.enqueue_xp_event(event)
            .await
            .map_err(storage_failure)?;
        self.apply_offline_xp(&event.event_id, lease)
            .await
            .map_err(storage_failure)
    }
    async fn owned_batch(&self, batch: &OwnedSaveBatch) -> Result<Vec<SaveAck>, SaveFailure> {
        self.save_owned_batch(batch).await.map_err(storage_failure)
    }
    async fn owned_routine(
        &self,
        lease: CharacterLease,
        snapshot: &SaveSnapshot,
    ) -> Result<SaveAck, SaveFailure> {
        match lease.state {
            OwnershipState::Offline => self.save_offline(lease, snapshot).await,
            _ => self.save_owned(lease, snapshot).await,
        }
        .map_err(storage_failure)
    }
    async fn close(&self) {
        PgStore::close(self).await;
    }
}
fn storage_failure(error: StoreError) -> SaveFailure {
    SaveFailure::Storage {
        uncertain: matches!(error, StoreError::CommitUncertain(_)),
        message: error.to_string(),
    }
}

pub struct SaveWorker {
    pub handle: SaveHandle,
    pub task: JoinHandle<SaveWorkerSummary>,
}
#[derive(Clone)]
pub struct SaveHandle {
    close: watch::Sender<bool>,
    offline: mpsc::Sender<Request>,
    offline_budget: Arc<Semaphore>,
    offline_limit: u32,
    routine: mpsc::Sender<Request>,
    valuable: mpsc::Sender<Request>,
    routine_budget: Arc<Semaphore>,
    valuable_budget: Arc<Semaphore>,
    routine_limit: u32,
    valuable_limit: u32,
}
struct Request {
    gameplay: Option<GameplayWrite>,
    owned_batch: Option<OwnedSaveBatch>,
    offline: Option<(OfflineXpEvent, CharacterLease)>,
    owner: Option<CharacterLease>,
    snapshots: Vec<SaveSnapshot>,
    operation_id: Option<String>,
    dirty_since: Option<Instant>,
    reply: oneshot::Sender<SaveReport>,
    _budget: OwnedSemaphorePermit,
    order: u64,
}
impl Request {
    fn deadline(&self) -> Option<Instant> {
        self.dirty_since
            .and_then(|since| since.checked_add(SAVE_INTERVAL))
    }
}
impl SaveHandle {
    /// Closes admission on the worker and drains already admitted work, including other clones.
    pub fn close(&self) {
        let _ = self.close.send(true);
    }

    pub fn try_offline(
        &self,
        event: &OfflineXpEvent,
        lease: CharacterLease,
        queued_since: Instant,
    ) -> Result<SaveTicket, SaveSubmitError> {
        if queued_since > Instant::now()
            || event.event_id.is_empty()
            || event.event_id.len() > 128
            || event.amount == 0
            || event.target_character != lease.character_id
            || lease.state != bace_persistence::OwnershipState::Offline
        {
            return Err(SaveSubmitError::Invalid);
        }
        let bytes =
            u32::try_from(event.event_id.len() + 32).map_err(|_| SaveSubmitError::Invalid)?;
        if bytes > self.offline_limit {
            return Err(SaveSubmitError::Invalid);
        }
        let slot = self.offline.try_reserve().map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => SaveSubmitError::Full,
            mpsc::error::TrySendError::Closed(_) => SaveSubmitError::Closed,
        })?;
        let permit = self
            .offline_budget
            .clone()
            .try_acquire_many_owned(bytes)
            .map_err(|_| SaveSubmitError::Full)?;
        let (reply, ticket) = oneshot::channel();
        slot.send(Request {
            gameplay: None,
            owned_batch: None,
            offline: Some((event.clone(), lease)),
            owner: None,
            snapshots: Vec::new(),
            operation_id: None,
            dirty_since: Some(queued_since),
            reply,
            _budget: permit,
            order: 0,
        });
        Ok(ticket)
    }
    pub fn try_owned_routine(
        &self,
        lease: CharacterLease,
        snapshot: &SaveSnapshot,
        dirty_since: Instant,
    ) -> Result<SaveTicket, SaveSubmitError> {
        if dirty_since > Instant::now()
            || snapshot.object_id != lease.character_id
            || !matches!(
                lease.state,
                OwnershipState::Online | OwnershipState::Offline
            )
        {
            return Err(SaveSubmitError::Invalid);
        }
        self.submit(
            std::slice::from_ref(snapshot),
            None,
            Some(dirty_since),
            Some(lease),
        )
    }

    /// Nonblocking backpressure. Input is borrowed, so refusal never consumes dirty state.
    pub fn try_routine(
        &self,
        snapshots: &[SaveSnapshot],
        dirty_since: Instant,
    ) -> Result<SaveTicket, SaveSubmitError> {
        if dirty_since > Instant::now() {
            return Err(SaveSubmitError::Invalid);
        }
        self.submit(snapshots, None, Some(dirty_since), None)
    }
    /// Participants MUST be reserved before submission; only a successful ticket releases success.
    pub fn try_valuable(
        &self,
        operation_id: &str,
        snapshots: &[SaveSnapshot],
    ) -> Result<SaveTicket, SaveSubmitError> {
        if operation_id.is_empty() || operation_id.len() > 128 {
            return Err(SaveSubmitError::Invalid);
        }
        self.submit(snapshots, Some(operation_id), None, None)
    }
    fn submit(
        &self,
        snapshots: &[SaveSnapshot],
        operation_id: Option<&str>,
        dirty_since: Option<Instant>,
        owner: Option<CharacterLease>,
    ) -> Result<SaveTicket, SaveSubmitError> {
        if snapshots.is_empty() || snapshots.len() > 1024 {
            return Err(SaveSubmitError::Invalid);
        }
        if snapshots
            .iter()
            .any(|snapshot| snapshot.bytes.is_empty() || snapshot.bytes.len() > 17 * 1024 * 1024)
        {
            return Err(SaveSubmitError::Invalid);
        }
        let bytes = snapshots
            .iter()
            .try_fold(0_usize, |sum, s| sum.checked_add(s.bytes.len()))
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(SaveSubmitError::Invalid)?;
        let (channel, budget, limit) = if operation_id.is_some() {
            (&self.valuable, &self.valuable_budget, self.valuable_limit)
        } else {
            (&self.routine, &self.routine_budget, self.routine_limit)
        };
        if bytes == 0 || bytes > limit || bytes > 64 * 1024 * 1024 {
            return Err(SaveSubmitError::Invalid);
        }
        let slot = channel.try_reserve().map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => SaveSubmitError::Full,
            mpsc::error::TrySendError::Closed(_) => SaveSubmitError::Closed,
        })?;
        let permit = budget
            .clone()
            .try_acquire_many_owned(bytes)
            .map_err(|_| SaveSubmitError::Full)?;
        let (reply, ticket) = oneshot::channel();
        slot.send(Request {
            gameplay: None,
            owned_batch: None,
            offline: None,
            owner,
            snapshots: snapshots.to_vec(),
            operation_id: operation_id.map(str::to_owned),
            dirty_since,
            reply,
            _budget: permit,
            order: 0,
        });
        Ok(ticket)
    }
}

/// Production factory reserves a one-connection pool exclusively for writes.
/// Migrations are a startup/administrative prerequisite, never work on the save queue.
pub async fn spawn_postgres_save_worker(
    url: &str,
    config: SaveWorkerConfig,
) -> Result<SaveWorker, StoreError> {
    validate_config(&config).map_err(|_| StoreError::Invalid("save worker configuration"))?;
    let store = PgStore::connect_writer(url, config.operation_timeout).await?;
    // Validated above, so this cannot discard an open pool due to configuration failure.
    spawn_save_worker(store, config).map_err(|_| StoreError::Invalid("save worker configuration"))
}
pub fn spawn_save_worker<B: SaveBackend>(
    backend: B,
    config: SaveWorkerConfig,
) -> Result<SaveWorker, SaveSubmitError> {
    validate_config(&config)?;
    let (close_tx, close_rx) = watch::channel(false);
    let (offline_tx, offline_rx) = mpsc::channel(config.offline_capacity);
    let (routine_tx, routine_rx) = mpsc::channel(config.routine_capacity);
    let (valuable_tx, valuable_rx) = mpsc::channel(config.valuable_capacity);
    let handle = SaveHandle {
        close: close_tx,
        offline: offline_tx,
        offline_budget: Arc::new(Semaphore::new(config.offline_bytes as usize)),
        offline_limit: config.offline_bytes,
        routine: routine_tx,
        valuable: valuable_tx,
        routine_budget: Arc::new(Semaphore::new(config.routine_bytes as usize)),
        valuable_budget: Arc::new(Semaphore::new(config.valuable_bytes as usize)),
        routine_limit: config.routine_bytes,
        valuable_limit: config.valuable_bytes,
    };
    let task = tokio::spawn(run(
        backend,
        config,
        routine_rx,
        valuable_rx,
        offline_rx,
        close_rx,
    ));
    Ok(SaveWorker { handle, task })
}
fn validate_config(config: &SaveWorkerConfig) -> Result<(), SaveSubmitError> {
    if !(1..=4096).contains(&config.offline_capacity)
        || config.offline_bytes == 0
        || config.offline_bytes > 256 * 1024 * 1024
        || !(1..=4096).contains(&config.routine_capacity)
        || !(1..=4096).contains(&config.valuable_capacity)
        || !(1..=64).contains(&config.maximum_valuable_burst)
        || !(1..=64).contains(&config.admission_per_turn)
        || config.routine_bytes == 0
        || config.valuable_bytes == 0
        || config.routine_bytes > 256 * 1024 * 1024
        || config.valuable_bytes > 256 * 1024 * 1024
        || config.operation_timeout < Duration::from_millis(10)
        || config.operation_timeout > Duration::from_secs(60)
    {
        return Err(SaveSubmitError::Invalid);
    }
    Ok(())
}

async fn run<B: SaveBackend>(
    backend: B,
    config: SaveWorkerConfig,
    mut routine_rx: mpsc::Receiver<Request>,
    mut valuable_rx: mpsc::Receiver<Request>,
    mut offline_rx: mpsc::Receiver<Request>,
    mut close: watch::Receiver<bool>,
) -> SaveWorkerSummary {
    let mut offlines = VecDeque::new();
    let mut prefer_offline = false;
    let mut routines = VecDeque::new();
    let mut valuables = VecDeque::new();
    let mut next_order = 0_u64;
    let mut burst = 0_usize;
    let mut summary = SaveWorkerSummary::default();
    loop {
        if *close.borrow() {
            routine_rx.close();
            valuable_rx.close();
            offline_rx.close();
        }
        admit(
            &mut routine_rx,
            &mut routines,
            config.routine_capacity,
            config.admission_per_turn,
            &mut next_order,
        );
        admit(
            &mut valuable_rx,
            &mut valuables,
            config.valuable_capacity,
            config.admission_per_turn,
            &mut next_order,
        );
        admit(
            &mut offline_rx,
            &mut offlines,
            config.offline_capacity,
            config.admission_per_turn,
            &mut next_order,
        );
        let closing = routine_rx.is_closed() && valuable_rx.is_closed() && offline_rx.is_closed();
        let oldest = routines
            .iter()
            .enumerate()
            .min_by_key(|(_, request)| (request.deadline(), request.order))
            .map(|(index, _)| index);
        let oldest_offline = offlines
            .iter()
            .enumerate()
            .min_by_key(|(_, r)| (r.deadline(), r.order))
            .map(|(i, _)| i);
        let routine_due = oldest.is_some_and(|i| {
            closing || routines[i].deadline().is_some_and(|d| d <= Instant::now())
        });
        let offline_due = oldest_offline.is_some_and(|i| {
            closing || offlines[i].deadline().is_some_and(|d| d <= Instant::now())
        });
        let background = (routine_due || offline_due) && burst >= config.maximum_valuable_burst
            || valuables.is_empty();
        let request = if background && (oldest.is_some() || oldest_offline.is_some()) {
            burst = 0;
            // Each noncritical lane gets a positive service quantum under sustained competing load.
            let choose_offline = oldest_offline.is_some()
                && (oldest.is_none()
                    || (offline_due && !routine_due)
                    || (prefer_offline && (offline_due || !routine_due)));
            prefer_offline = !choose_offline;
            if choose_offline {
                offlines.remove(oldest_offline.unwrap_or(0))
            } else {
                routines.remove(oldest.unwrap_or(0))
            }
        } else if let Some(request) = valuables.pop_front() {
            burst = burst.saturating_add(1).min(config.maximum_valuable_burst);
            Some(request)
        } else {
            None
        };
        if let Some(request) = request {
            let late = request.deadline().map_or(Duration::ZERO, |deadline| {
                Instant::now().saturating_duration_since(deadline)
            });
            let operation = async {
                if let Some(gameplay) = &request.gameplay {
                    match gameplay {
                        GameplayWrite::AllegiancePlacement(operation) => {
                            backend.allegiance_placement(operation).await
                        }
                        GameplayWrite::Allegiance(operation) => {
                            return backend
                                .allegiance(operation)
                                .await
                                .map(WriteOutcome::Allegiance);
                        }
                        GameplayWrite::Inventory(operation) => backend.inventory(operation).await,
                        GameplayWrite::NpcStage(operation) => backend.npc_stage(operation).await,
                        GameplayWrite::Placement(operation) => backend.placement(operation).await,
                        GameplayWrite::WorldPlacement(operation) => {
                            backend.world_placement(operation).await
                        }
                        GameplayWrite::VendorStock(operation) => {
                            backend.vendor_stock(operation).await
                        }
                        GameplayWrite::ConstructedCreaturePromotion(operation) => {
                            backend.constructed_creature_promotion(operation).await
                        }
                        GameplayWrite::HousingLifecycle(operation) => {
                            backend.housing_operation(operation).await
                        }
                        GameplayWrite::Housing(operation) => backend.housing(operation).await,
                    }
                    .map(WriteOutcome::Valuable)
                } else if let Some(batch) = &request.owned_batch {
                    backend.owned_batch(batch).await.map(WriteOutcome::Routine)
                } else if let Some((event, lease)) = &request.offline {
                    backend
                        .offline(event, *lease)
                        .await
                        .map(WriteOutcome::Offline)
                } else if let Some(lease) = request.owner {
                    backend
                        .owned_routine(lease, &request.snapshots[0])
                        .await
                        .map(|ack| WriteOutcome::Routine(vec![ack]))
                } else if let Some(id) = &request.operation_id {
                    backend
                        .valuable(id, &request.snapshots)
                        .await
                        .map(WriteOutcome::Valuable)
                } else {
                    backend
                        .routine(&request.snapshots)
                        .await
                        .map(WriteOutcome::Routine)
                }
            };
            let result = tokio::time::timeout(config.operation_timeout, operation)
                .await
                .unwrap_or(Err(SaveFailure::Timeout));
            match &result {
                Ok(WriteOutcome::Offline(_)) => summary.offline_completed += 1,
                Ok(WriteOutcome::Routine(_)) => summary.routine_completed += 1,
                Ok(WriteOutcome::Valuable(_) | WriteOutcome::Allegiance(_)) => {
                    summary.valuable_completed += 1
                }
                Err(_) => summary.failed += 1,
            }
            // Even a dropped receiver does not cancel an admitted durable write.
            let _ = request.reply.send(SaveReport {
                result,
                dirty_since: request.dirty_since,
                started_late_by: late,
            });
            drop(request.snapshots);
            drop(request._budget);
            tokio::task::yield_now().await;
            continue;
        }
        if closing
            && routine_rx.capacity() == routine_rx.max_capacity()
            && valuable_rx.capacity() == valuable_rx.max_capacity()
            && offline_rx.capacity() == offline_rx.max_capacity()
        {
            break;
        }
        // Receive at most one message before rechecking deadlines and scheduling. No unbounded drain loop.
        tokio::select! {
            _=close.changed(), if !closing => {}
            value=offline_rx.recv(), if offlines.len()<config.offline_capacity&&(!offline_rx.is_closed()||offline_rx.capacity()<offline_rx.max_capacity()) => {
                if let Some(mut request)=value {request.order=next_order;next_order=next_order.saturating_add(1);offlines.push_back(request);}
            }
            value=routine_rx.recv(), if routines.len()<config.routine_capacity&&(!routine_rx.is_closed()||routine_rx.capacity()<routine_rx.max_capacity()) => {
                if let Some(mut request)=value {request.order=next_order;next_order=next_order.saturating_add(1);routines.push_back(request);}
            }
            value=valuable_rx.recv(), if valuables.len()<config.valuable_capacity&&(!valuable_rx.is_closed()||valuable_rx.capacity()<valuable_rx.max_capacity()) => {
                if let Some(mut request)=value {request.order=next_order;next_order=next_order.saturating_add(1);valuables.push_back(request);}
            }
        }
    }
    summary.shutdown_timed_out = tokio::time::timeout(config.operation_timeout, backend.close())
        .await
        .is_err();
    summary
}
fn admit(
    receiver: &mut mpsc::Receiver<Request>,
    pending: &mut VecDeque<Request>,
    capacity: usize,
    budget: usize,
    order: &mut u64,
) {
    for _ in 0..budget.min(capacity.saturating_sub(pending.len())) {
        let Ok(mut request) = receiver.try_recv() else {
            break;
        };
        request.order = *order;
        *order = order.saturating_add(1);
        pending.push_back(request);
    }
}
