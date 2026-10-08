//! Bounded cold identity resolution. Keep subsequent actions for a pending session
//! behind this ingress barrier until `flush` admits its exact resolved command.
use bace_db_postgres::{PgStore, PlayerIdentity, PlayerIdentityQuery};
use bace_gameplay_api::{
    ActionContext, SessionId,
    social::{SocialIdentity, SocialRequest},
};
use bace_simulation::Command;
use bace_types::{AccountId, EntityId};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::JoinHandle,
    time::Duration,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SocialLookupAdmissionError {
    #[error("invalid social identity lookup request")]
    Invalid,
    #[error("social lookup ingress backpressure")]
    Capacity,
    #[error("social lookup worker stopped")]
    Closed,
    #[error("social lookup correlation exhausted")]
    CorrelationExhausted,
}
#[derive(Clone, Debug)]
pub struct PendingSocialAction {
    pub context: ActionContext,
    pub request: SocialRequest,
}
struct Work {
    correlation: u64,
    query: PlayerIdentityQuery,
}
struct Completion {
    correlation: u64,
    result: Result<Option<PlayerIdentity>, String>,
}
struct Pending {
    correlation: u64,
    action: PendingSocialAction,
    query: PlayerIdentityQuery,
    ready: Option<Command>,
    failure: Option<String>,
}
pub struct SocialLookupService {
    sender: SyncSender<Work>,
    receiver: Receiver<Completion>,
    worker: JoinHandle<()>,
    stop: Arc<AtomicBool>,
    pending: BTreeMap<SessionId, Pending>,
    capacity: usize,
    next: u64,
}
impl SocialLookupService {
    pub fn start(store: PgStore, capacity: usize) -> Result<Self, String> {
        Self::start_with_pressure(store, capacity, Arc::new(AtomicBool::new(false)))
    }
    pub fn start_with_pressure(
        store: PgStore,
        capacity: usize,
        save_pressure: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        if !(1..=4096).contains(&capacity) {
            return Err("social lookup capacity outside bounds".into());
        }
        let (sender, work) = mpsc::sync_channel::<Work>(capacity);
        let (completed, receiver) = mpsc::sync_channel(capacity);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = std::thread::Builder::new()
            .name("bace-social-lookup".into())
            .spawn(move || {
                while let Ok(work) = work.recv() {
                    // SQL reads yield while the save scheduler has deadline debt.
                    // Cancellation remains bounded even if that pressure persists.
                    while save_pressure.load(Ordering::Acquire)
                        && !worker_stop.load(Ordering::Acquire)
                    {
                        std::thread::park_timeout(Duration::from_millis(20));
                    }
                    if worker_stop.load(Ordering::Acquire) {
                        break;
                    }
                    let result = runtime.block_on(async {
                        tokio::time::timeout(
                            Duration::from_secs(5),
                            store.lookup_player_identity(&work.query),
                        )
                        .await
                        .map_err(|_| "social identity read timed out".to_string())?
                        .map_err(|e| e.to_string())
                    });
                    if completed
                        .send(Completion {
                            correlation: work.correlation,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            sender,
            receiver,
            worker,
            stop,
            pending: BTreeMap::new(),
            capacity,
            next: 0,
        })
    }
    pub fn pending_session(&self, session: SessionId) -> bool {
        self.pending.contains_key(&session)
    }
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn begin(
        &mut self,
        action: PendingSocialAction,
    ) -> Result<(), (SocialLookupAdmissionError, Box<PendingSocialAction>)> {
        let query = match query(&action.request) {
            Ok(query) => query,
            Err(_) => return Err((SocialLookupAdmissionError::Invalid, Box::new(action))),
        };
        if self.pending.len() >= self.capacity || self.pending_session(action.context.session) {
            return Err((SocialLookupAdmissionError::Capacity, Box::new(action)));
        }
        let Some(correlation) = self.next.checked_add(1) else {
            return Err((
                SocialLookupAdmissionError::CorrelationExhausted,
                Box::new(action),
            ));
        };
        if let Err(error) = self.sender.try_send(Work {
            correlation,
            query: query.clone(),
        }) {
            let error = match error {
                mpsc::TrySendError::Full(_) => SocialLookupAdmissionError::Capacity,
                mpsc::TrySendError::Disconnected(_) => SocialLookupAdmissionError::Closed,
            };
            return Err((error, Box::new(action)));
        }
        self.next = correlation;
        self.pending.insert(
            action.context.session,
            Pending {
                correlation,
                action,
                query,
                ready: None,
                failure: None,
            },
        );
        Ok(())
    }
    pub fn poll(&mut self, budget: usize) -> Result<usize, String> {
        if !(1..=4096).contains(&budget) {
            return Err("social lookup poll budget outside bounds".into());
        }
        let mut count = 0;
        for _ in 0..budget {
            let completion = match self.receiver.try_recv() {
                Ok(value) => value,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("social lookup worker stopped; pending actions retained".into());
                }
            };
            let pending = self
                .pending
                .values_mut()
                .find(|p| p.correlation == completion.correlation)
                .ok_or("unknown social lookup completion")?;
            if pending.ready.is_some() || pending.failure.is_some() {
                return Err("duplicate social lookup completion".into());
            }
            match completion.result {
                Err(error) => pending.failure = Some(error),
                Ok(identity) => {
                    pending.ready = Some(Command::SocialResolved {
                        context: pending.action.context,
                        request: pending.action.request.clone(),
                        identity: identity.map(|identity| SocialIdentity {
                            character: EntityId(identity.object_id),
                            account: AccountId(identity.account_id),
                            name: identity.name,
                        }),
                    })
                }
            }
            count += 1;
        }
        Ok(count)
    }
    pub fn failure(&self, session: SessionId) -> Option<&str> {
        self.pending.get(&session)?.failure.as_deref()
    }
    pub fn retry(&mut self, session: SessionId) -> Result<(), String> {
        let pending = self
            .pending
            .get_mut(&session)
            .ok_or("no pending social lookup")?;
        if pending.failure.is_none() {
            return Err("social lookup is not retryable".into());
        }
        self.sender
            .try_send(Work {
                correlation: pending.correlation,
                query: pending.query.clone(),
            })
            .map_err(|_| "social lookup retry backpressure")?;
        pending.failure = None;
        Ok(())
    }
    pub fn flush(
        &mut self,
        budget: usize,
        mut send: impl FnMut(Command) -> Result<(), Box<Command>>,
    ) -> usize {
        let ready: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, p)| p.ready.is_some())
            .map(|(id, _)| *id)
            .take(budget.min(4096))
            .collect();
        let mut count = 0;
        for session in ready {
            let pending = self
                .pending
                .get_mut(&session)
                .expect("selected pending session");
            let command = pending.ready.take().expect("selected ready command");
            if let Err(command) = send(command) {
                pending.ready = Some(*command);
                break;
            }
            self.pending.remove(&session);
            count += 1;
        }
        count
    }
    /// Lifecycle-only blocking join. Read cancellation never adopts an action;
    /// every unadmitted action is returned for recovery in original session order.
    pub fn stop(self) -> (Vec<PendingSocialAction>, Result<(), String>) {
        self.stop.store(true, Ordering::Release);
        self.worker.thread().unpark();
        let Self {
            sender,
            receiver,
            worker,
            pending,
            ..
        } = self;
        drop(sender);
        drop(receiver);
        let actions = pending.into_values().map(|p| p.action).collect();
        (
            actions,
            worker
                .join()
                .map_err(|_| "social lookup worker panicked".into()),
        )
    }
}
fn query(request: &SocialRequest) -> Result<PlayerIdentityQuery, String> {
    let query = match request {
        SocialRequest::AddFriend(name) | SocialRequest::AccountSquelch { name, .. } => {
            PlayerIdentityQuery::Name(name.clone())
        }
        SocialRequest::CharacterSquelch { target, name, .. } => {
            if target.0 == 0 {
                PlayerIdentityQuery::Name(name.clone())
            } else {
                PlayerIdentityQuery::Character(target.0)
            }
        }
        _ => return Err("request does not require offline identity resolution".into()),
    };
    if let PlayerIdentityQuery::Character(id) = query
        && !(0x50000001..=0x5fffffff).contains(&id)
    {
        return Err("invalid social lookup character".into());
    }
    if let PlayerIdentityQuery::Name(name) = &query
        && (name.is_empty() || name.len() > 100 || name.contains('\0'))
    {
        return Err("invalid social lookup name".into());
    }
    Ok(query)
}

#[cfg(test)]
#[path = "social_lookup_tests.rs"]
mod tests;
