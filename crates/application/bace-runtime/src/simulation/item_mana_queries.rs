//! One bounded current-item-mana read in transit; shutdown retains delivery.
use super::*;
use bace_simulation::ItemManaQueryOutcome;

#[derive(Default)]
pub struct RecoveredItemManaQueries {
    pub outcomes: Vec<ItemManaQueryOutcome>,
}

impl RecoveredItemManaQueries {
    pub(super) fn has_state(&self) -> bool {
        !self.outcomes.is_empty()
    }
}

pub(super) struct ItemManaQueryReceivers {
    outcomes: mpsc::Receiver<ItemManaQueryOutcome>,
}

pub(super) struct ItemManaQuerySenders {
    outcomes: mpsc::SyncSender<ItemManaQueryOutcome>,
}

pub(super) fn channels(capacity: usize) -> (ItemManaQuerySenders, ItemManaQueryReceivers) {
    let (sender, outcomes) = mpsc::sync_channel(capacity);
    (
        ItemManaQuerySenders { outcomes: sender },
        ItemManaQueryReceivers { outcomes },
    )
}

impl ItemManaQuerySenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.peek_item_mana_query_outcome() else {
                break;
            };
            if self.outcomes.try_send(*outcome).is_err() {
                break;
            }
            kernel.take_item_mana_query_outcome();
        }
    }
}

impl ItemManaQueryReceivers {
    pub(super) fn recover(&self, output: &mut RecoveredItemManaQueries) {
        output.outcomes.extend(self.outcomes.try_iter());
    }
}

impl SimulationWorker {
    pub fn item_mana_query_outcomes(&self) -> &mpsc::Receiver<ItemManaQueryOutcome> {
        &self.item_mana_queries.outcomes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_gameplay_api::{CharacterBinding, SessionId};
    use bace_simulation::ItemManaQueryRequest;
    use bace_types::{AccountId, EntityId};

    #[test]
    fn bounded_channel_drains_a_batch_and_retains_the_remainder() {
        let mut kernel =
            Kernel::with_gameplay_limits(bace_world::World::default(), 128, 1, 64).unwrap();
        let binding = CharacterBinding {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(1),
        };
        for correlation in 1..=64 {
            kernel
                .enqueue(Command::ItemManaQuery(ItemManaQueryRequest {
                    correlation,
                    binding,
                    target: EntityId(2),
                }))
                .unwrap();
        }
        kernel.step().unwrap();
        let (sender, receiver) = channels(32);
        sender.flush(&mut kernel, 64);
        let first: Vec<_> = receiver.outcomes.try_iter().collect();
        assert_eq!(first.len(), 32);
        assert_eq!(first[0].correlation, 1);
        assert_eq!(first[31].correlation, 32);
        assert!(kernel.has_item_mana_query_work());
        sender.flush(&mut kernel, 64);
        let second: Vec<_> = receiver.outcomes.try_iter().collect();
        assert_eq!(second.len(), 32);
        assert_eq!(second[0].correlation, 33);
        assert_eq!(second[31].correlation, 64);
        assert!(!kernel.has_item_mana_query_work());
    }
}
