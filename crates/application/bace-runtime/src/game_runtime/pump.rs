//! One fixed adapter loop. Control closure/pause returns to its caller with the
//! entire borrowed runtime intact; it is never permission to drop a live world.
use super::*;
use tokio::sync::mpsc;

#[derive(Clone, Copy, Debug)]
pub enum GameRuntimeControl {
    Quiesce,
    RetryPlayer(SessionKey),
    /// Pause this adapter future without transferring or destroying ownership.
    Pause,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameRuntimePause {
    Requested,
    ControlClosed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameRuntimeStatus {
    pub quiescing: bool,
    pub sessions: usize,
    pub loading: usize,
    pub retained_messages: usize,
    pub retained_bytes: usize,
    pub requires_drain: bool,
    pub failure: Option<String>,
}
impl GameRuntime {
    pub fn poll_now(&mut self) -> Result<(), String> {
        self.poll(self.clock.monotonic.elapsed())
    }
    pub fn quiesce_now(&mut self) -> Result<(), String> {
        self.quiesce(self.clock.monotonic.elapsed())
    }
    pub(crate) async fn verify_world_owner(&mut self) -> Result<(), String> {
        match tokio::time::timeout(Duration::from_secs(1), self.bootstrap.world_owner.check()).await
        {
            Ok(Ok(())) => Ok(()),
            _ => {
                self.quiesce_now()?;
                Err("database world ownership could not be verified; admission stopped".into())
            }
        }
    }
    pub fn status(&self) -> GameRuntimeStatus {
        GameRuntimeStatus {
            quiescing: self.draining,
            sessions: self.sessions.len(),
            loading: self
                .sessions
                .values()
                .filter(|s| {
                    s.loading
                        .as_ref()
                        .is_some_and(|l| l.phase != lifecycle::Phase::Entered)
                })
                .count(),
            retained_messages: self.input.len(),
            retained_bytes: self.input_bytes,
            requires_drain: self.requires_drain(),
            failure: self.failure.clone(),
        }
    }
    /// Runs on the single adapter executor. A five-millisecond service cadence
    /// polls only bounded work; SQL futures and asset workers remain independent
    /// of the simulation owner. Missed turns are skipped rather than burst-replayed.
    ///
    /// The caller owns `self` across cancellation and every return. Even after a
    /// quiesce request, only a complete durable shutdown proof can release the
    /// database world lease. This method never claims such a proof itself.
    pub async fn run_until_paused(
        &mut self,
        controls: &mut mpsc::Receiver<GameRuntimeControl>,
    ) -> GameRuntimePause {
        let mut turns = tokio::time::interval(Duration::from_millis(5));
        turns.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                command=controls.recv()=>{
                    match command {
                        Some(GameRuntimeControl::Pause)=>return GameRuntimePause::Requested,
                        None=>return GameRuntimePause::ControlClosed,
                        Some(GameRuntimeControl::Quiesce)=>{
                            if let Err(error)=self.quiesce(self.clock.monotonic.elapsed()) {
                                self.failure=Some(error);
                            }
                        }
                        Some(GameRuntimeControl::RetryPlayer(key))=>{
                            if let Err(error)=self.retry_player(key) {
                                self.failure=Some(error);
                            }
                        }
                    }
                }
                _=turns.tick()=>{
                    // poll records the degraded diagnostic and retains all work.
                    // Continue serving independent durable/lifecycle lanes.
                    if let Err(error)=self.poll(self.clock.monotonic.elapsed()) {
                        self.failure=Some(error);
                    }
                }
            }
        }
    }
}
