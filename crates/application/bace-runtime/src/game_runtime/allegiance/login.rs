//! One login redemption request per entered binding. A prepared simulation
//! proposal is not a completed reward until its exact durable receipt returns.
use bace_gameplay_api::{CharacterBinding, social::SocialError};
use bace_session::SessionKey;
use bace_simulation::SocialControlOutcome;
use std::collections::BTreeMap;

type LoginOutcome = (
    SessionKey,
    CharacterBinding,
    Result<Option<u64>, SocialError>,
);

#[derive(Default)]
pub(super) struct LoginRedemptions {
    in_flight: Option<(u64, SessionKey, CharacterBinding)>,
    awaiting_commit: BTreeMap<SessionKey, (CharacterBinding, u64)>,
    complete: BTreeMap<SessionKey, CharacterBinding>,
}

impl LoginRedemptions {
    pub(super) fn has_pending(&self) -> bool {
        self.in_flight.is_some() || !self.awaiting_commit.is_empty()
    }

    pub(super) fn owns(&self, sequence: u64) -> bool {
        self.in_flight
            .is_some_and(|(expected, _, _)| expected == sequence)
    }

    /// `entered` comes from PlayerService's ordered session map, after filtering.
    pub(super) fn retain_entered(&mut self, entered: &[(SessionKey, CharacterBinding)]) {
        self.complete.retain(|key, binding| {
            entered
                .binary_search_by_key(key, |(candidate, _)| *candidate)
                .is_ok_and(|index| entered[index].1 == *binding)
        });
    }

    pub(super) fn candidate(
        &self,
        entered: &[(SessionKey, CharacterBinding)],
    ) -> Option<(SessionKey, CharacterBinding)> {
        if self.in_flight.is_some() {
            return None;
        }
        entered.iter().copied().find(|(key, binding)| {
            self.complete.get(key) != Some(binding) && !self.awaiting_commit.contains_key(key)
        })
    }

    pub(super) fn start(
        &mut self,
        sequence: u64,
        key: SessionKey,
        binding: CharacterBinding,
    ) -> Result<(), &'static str> {
        if sequence == 0
            || self.in_flight.is_some()
            || self.complete.get(&key) == Some(&binding)
            || self.awaiting_commit.contains_key(&key)
        {
            return Err("login redemption admission mismatch");
        }
        self.in_flight = Some((sequence, key, binding));
        Ok(())
    }

    pub(super) fn accept(
        &mut self,
        outcome: SocialControlOutcome,
    ) -> Result<LoginOutcome, &'static str> {
        let (sequence, key, binding) = self.in_flight.ok_or("login redemption owner missing")?;
        if outcome.sequence != sequence {
            return Err("login redemption correlation mismatch");
        }
        match outcome.result {
            Ok(Some(operation)) => {
                if operation == 0
                    || self
                        .awaiting_commit
                        .values()
                        .any(|(_, prior)| *prior == operation)
                {
                    return Err("login redemption operation mismatch");
                }
                self.awaiting_commit.insert(key, (binding, operation));
            }
            Ok(None) => {
                self.complete.insert(key, binding);
            }
            Err(_) => {}
        }
        self.in_flight = None;
        Ok((key, binding, outcome.result))
    }

    pub(super) fn finish(&mut self, operation: u64, committed: bool) -> bool {
        let Some((key, binding)) = self
            .awaiting_commit
            .iter()
            .find_map(|(key, value)| (value.1 == operation).then_some((*key, value.0)))
        else {
            return false;
        };
        self.awaiting_commit.remove(&key);
        if committed {
            self.complete.insert(key, binding);
        }
        true
    }
}

#[cfg(test)]
#[path = "login/tests.rs"]
mod tests;
