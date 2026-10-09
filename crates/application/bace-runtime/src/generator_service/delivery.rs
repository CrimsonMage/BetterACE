//! Exactly one retained command per participant; enqueue is not owner success.
use super::*;
#[derive(Clone, Copy)]
pub(super) enum Purpose {
    Seed,
    Discard,
    Reserve(GeneratorSpawnKey),
    Refresh(GeneratorSpawnKey),
    Admit(GeneratorSpawnKey),
    Retire,
}
pub(super) struct Delivery {
    pub correlation: u64,
    pub action: GeneratorAction,
    pub purpose: Purpose,
    pub submitted: bool,
}
impl Delivery {
    pub fn submit(&mut self, worker: &SimulationWorker) {
        if !self.submitted {
            let command = Command::Generator(GeneratorCommand {
                correlation: self.correlation,
                action: self.action.clone(),
            });
            if worker.input().try_submit(command).is_ok() {
                self.submitted = true;
            }
        }
    }
}
impl<S: GeneratorRepository> GeneratorService<S> {
    pub(super) fn delivery(
        &mut self,
        action: GeneratorAction,
        purpose: Purpose,
    ) -> Result<Delivery, String> {
        self.next = self
            .next
            .checked_add(1)
            .filter(|v| v >> 48 == PREFIX >> 48)
            .ok_or("generator correlation capacity")?;
        Ok(Delivery {
            correlation: self.next,
            action,
            purpose,
            submitted: false,
        })
    }
    pub(super) fn accept_outcome(
        &mut self,
        outcome: GeneratorCommandOutcome,
    ) -> Result<(), Box<GeneratorCommandOutcome>> {
        if !self.owns_generator_outcome(&outcome) {
            return Err(Box::new(outcome));
        }
        let purpose = self
            .seed
            .as_ref()
            .filter(|d| d.submitted && d.correlation == outcome.correlation)
            .map(|d| d.purpose)
            .or_else(|| {
                self.discard
                    .as_ref()
                    .filter(|d| d.submitted && d.correlation == outcome.correlation)
                    .map(|d| d.purpose)
            })
            .or_else(|| {
                self.jobs
                    .values()
                    .filter_map(|w| w.delivery.as_ref())
                    .find(|d| d.submitted && d.correlation == outcome.correlation)
                    .map(|d| d.purpose)
            })
            .or_else(|| {
                self.retirement
                    .as_ref()
                    .and_then(|r| r.delivery())
                    .filter(|d| d.submitted && d.correlation == outcome.correlation)
                    .map(|d| d.purpose)
            });
        let Some(purpose) = purpose else {
            if self.unexpected_outcome.is_some() {
                return Err(Box::new(outcome));
            }
            self.unexpected_outcome = Some(outcome);
            self.fault = Some("unexpected generator command outcome retained".into());
            return Ok(());
        };
        match purpose {
            Purpose::Seed => match outcome.result {
                Ok(()) => {
                    self.seed = None;
                    self.spare_ids.pop_front();
                }
                Err(GeneratorServiceError::Capacity | GeneratorServiceError::Busy) => {
                    self.seed.as_mut().unwrap().submitted = false;
                }
                Err(e) => {
                    self.seed.as_mut().unwrap().submitted = false;
                    self.fault = Some(format!("generator identity admission: {e:?}"));
                }
            },
            Purpose::Discard => match outcome.result {
                Ok(()) => {
                    self.discard = None;
                    self.discarded = true;
                }
                Err(GeneratorServiceError::Busy | GeneratorServiceError::Capacity) => {
                    self.discard.as_mut().unwrap().submitted = false
                }
                Err(e) => {
                    self.discard.as_mut().unwrap().submitted = false;
                    self.fault = Some(format!("generator identity drain: {e:?}"));
                }
            },
            Purpose::Retire => {
                self.retirement.as_mut().unwrap().accept(outcome);
            }
            Purpose::Reserve(key) | Purpose::Refresh(key) | Purpose::Admit(key) => {
                let work = self.jobs.get_mut(&key).unwrap();
                match outcome.result {
                    Ok(()) => {
                        match purpose {
                            Purpose::Reserve(_) | Purpose::Refresh(_) => {
                                let Some(request) = outcome.request else {
                                    work.blocked = Some(
                                        "generator refresh receipt has no owner request".into(),
                                    );
                                    return Ok(());
                                };
                                if request.intent != work.request.intent
                                    || request.landblock != work.request.landblock
                                    || !request.entities.starts_with(&work.request.entities)
                                {
                                    work.blocked =
                                        Some("generator refresh receipt identity mismatch".into());
                                    return Ok(());
                                }
                                if matches!(purpose, Purpose::Reserve(_))
                                    && request.entities[work.request.entities.len()..] != work.ids
                                {
                                    work.blocked =
                                        Some("generator reserved identity tail mismatch".into());
                                    return Ok(());
                                }
                                work.request = request;
                                work.ids.clear();
                                work.phase = Phase::Ids;
                                work.slot_refreshed = matches!(purpose, Purpose::Refresh(_));
                            }
                            Purpose::Admit(_) => {
                                let action = work.ready.as_ref().unwrap().action.spawn_action();
                                if matches!(
                                    action,
                                    GeneratorAction::AdmitItemTrees { .. }
                                        | GeneratorAction::AdmitStockTrees { .. }
                                        | GeneratorAction::AdmitMixedTrees { .. }
                                        | GeneratorAction::AdmitContainedCreature { .. }
                                        | GeneratorAction::AdmitContainedForest { .. }
                                ) {
                                    let Some(admitted) = outcome.admission else {
                                        work.blocked =
                                            Some("generator tree admission receipt missing".into());
                                        return Ok(());
                                    };
                                    if !(valid_tree_receipt(action, &admitted)
                                        || super::stock::valid_receipt(action, &admitted)
                                        || super::mixed::valid_receipt(action, &admitted))
                                    {
                                        work.blocked = Some(
                                            "generator tree admission receipt mismatch".into(),
                                        );
                                        return Ok(());
                                    }
                                    work.failed_sources =
                                        failed_sources(&work.source_ids, &admitted);
                                }
                                work.phase = Phase::Complete;
                            }
                            _ => {}
                        }
                        work.delivery = None;
                    }
                    Err(GeneratorServiceError::Busy | GeneratorServiceError::Capacity) => {
                        work.delivery.as_mut().unwrap().submitted = false;
                    }
                    Err(GeneratorServiceError::Placement)
                        if matches!(purpose, Purpose::Admit(_))
                            && matches!(
                                work.ready.as_ref().map(|r| r.action.spawn_action()),
                                Some(GeneratorAction::AdmitCreature { .. })
                            ) =>
                    {
                        // The physical NPC root has a public description but no
                        // RegionItemSource row; Placement produced no Spawned
                        // event to consume that description.
                        work.failed_sources = work
                            .source_ids
                            .iter()
                            .copied()
                            .chain(work.publications.iter().map(|p| p.entity))
                            .collect::<std::collections::BTreeSet<_>>()
                            .into_iter()
                            .collect();
                        work.ready = Some(materialization::Ready {
                            action: GeneratorAction::SpawnReceipt(
                                bace_gameplay_api::GeneratorSpawnReceipt {
                                    key,
                                    result: bace_gameplay_api::GeneratorSpawnResult::Completed {
                                        members: vec![],
                                        materialized: true,
                                        failed_placements: 1,
                                    },
                                },
                            ),
                            sources: vec![],
                        });
                        work.delivery = None;
                    }
                    Err(e) => {
                        work.delivery.as_mut().unwrap().submitted = false;
                        work.blocked = Some(format!("generator admission: {e:?}"));
                    }
                }
            }
        }
        Ok(())
    }
}

/// A failed physical root can have no inventory-source row (a plain NPC), but
/// its prepared public description still needs the exact rejected-root receipt.
/// Retain every accepted root for its later Spawned publication handoff.
pub(super) fn failed_sources(
    source_ids: &[EntityId],
    admitted: &bace_simulation::GeneratorItemAdmission,
) -> Vec<EntityId> {
    let accepted: std::collections::BTreeSet<_> = admitted.entities.iter().copied().collect();
    source_ids
        .iter()
        .copied()
        .filter(|id| !accepted.contains(id))
        .chain(admitted.failed_roots.iter().copied())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Every source root has exactly one result, and a successful root owns its whole
/// descendant graph. A partial/mismatched acknowledgment cannot evict cold data.
pub(super) fn valid_tree_receipt(
    action: &GeneratorAction,
    receipt: &bace_simulation::GeneratorItemAdmission,
) -> bool {
    use std::collections::{BTreeMap, BTreeSet};
    if matches!(action, GeneratorAction::AdmitContainedForest { .. }) {
        return super::contained::valid_receipt(action, receipt);
    }
    if let GeneratorAction::AdmitContainedCreature { key, prepared } = action {
        let expected: BTreeSet<_> = std::iter::once(prepared.root.id)
            .chain(prepared.loadout.items.iter().map(|i| i.id))
            .collect();
        return receipt.key == *key
            && receipt.roots == [prepared.root.id]
            && receipt.failed_roots.is_empty()
            && receipt.entities.len() == expected.len()
            && receipt.entities.iter().copied().collect::<BTreeSet<_>>() == expected;
    }
    let GeneratorAction::AdmitItemTrees {
        key, items, roots, ..
    } = action
    else {
        return false;
    };
    let accepted: BTreeSet<_> = receipt.roots.iter().copied().collect();
    let failed: BTreeSet<_> = receipt.failed_roots.iter().copied().collect();
    let roots: BTreeSet<_> = roots.iter().copied().collect();
    let entities: BTreeSet<_> = receipt.entities.iter().copied().collect();
    if receipt.key != *key
        || accepted.len() != receipt.roots.len()
        || failed.len() != receipt.failed_roots.len()
        || entities.len() != receipt.entities.len()
        || !accepted.is_disjoint(&failed)
        || accepted.union(&failed).copied().collect::<BTreeSet<_>>() != roots
    {
        return false;
    }
    let items: BTreeMap<_, _> = items.iter().map(|i| (i.id, i)).collect();
    let mut expected = BTreeSet::new();
    for &id in items.keys() {
        let mut parent = id;
        for _ in 0..=items.len() {
            if roots.contains(&parent) {
                break;
            }
            let Some(item) = items.get(&parent) else {
                return false;
            };
            let bace_inventory::ItemPlace::Contained { container, .. } = item.place else {
                return false;
            };
            parent = container;
        }
        if !roots.contains(&parent) {
            return false;
        }
        if accepted.contains(&parent) {
            expected.insert(id);
        }
    }
    entities == expected
}
