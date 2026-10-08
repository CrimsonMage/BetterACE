//! One correlated clock command at a time; queue admission is not owner success.
use bace_simulation::{Command, GeneratorAction, GeneratorCommand, GeneratorCommandOutcome};
const CORRELATION: u64 = u64::MAX;
#[derive(Default)]
pub(super) struct ClockDelivery {
    applied: Option<bool>,
    inflight: Option<bool>,
    pending: Option<bool>,
    failure: Option<GeneratorCommandOutcome>,
}
impl ClockDelivery {
    pub fn update(&mut self, day: bool) {
        if self.inflight.is_none() && self.failure.is_none() {
            self.pending = (self.applied != Some(day)).then_some(day);
        }
    }
    pub fn flush(&mut self, mut submit: impl FnMut(Command) -> bool) {
        if let Some(day) = self.pending {
            let command = Command::Generator(GeneratorCommand {
                correlation: CORRELATION,
                action: GeneratorAction::SetDay(day),
            });
            if submit(command) {
                self.pending = None;
                self.inflight = Some(day);
            }
        }
    }
    pub fn owns(outcome: &GeneratorCommandOutcome) -> bool {
        outcome.correlation == CORRELATION
    }
    pub fn accept(
        &mut self,
        outcome: GeneratorCommandOutcome,
    ) -> Result<(), Box<GeneratorCommandOutcome>> {
        if !Self::owns(&outcome) || self.failure.is_some() {
            return Err(Box::new(outcome));
        }
        if outcome.result.is_err() || self.inflight.is_none() {
            self.failure = Some(outcome);
            return Ok(());
        }
        self.applied = self.inflight.take();
        Ok(())
    }
    pub fn failure(&self) -> Option<&GeneratorCommandOutcome> {
        self.failure.as_ref()
    }
    pub fn retry(&mut self) {
        if self.failure.take().is_some() {
            self.inflight = None;
        }
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some() || self.inflight.is_some() || self.failure.is_some()
    }
}
#[cfg(test)]
mod tests;
