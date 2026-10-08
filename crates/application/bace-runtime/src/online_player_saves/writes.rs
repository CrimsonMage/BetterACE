//! Routine submission and uncertain CAS reconciliation retain the exact payload.
use super::*;
impl OnlinePlayerSaveService {
    pub fn submit_due(
        &mut self,
        handle: &SaveHandle,
        now: Duration,
        draining: bool,
        budget: usize,
    ) -> Result<usize, String> {
        let ready = if draining {
            self.dirty.drain_ready()
        } else {
            self.dirty.due(now)
        };
        let mut grouped = BTreeMap::<u32, Vec<SaveSnapshot>>::new();
        for request in ready {
            let actor = if self.players.contains_key(&request.object_id) {
                request.object_id
            } else {
                self.items
                    .get(&request.object_id)
                    .ok_or("routine item owner missing")?
                    .owner
            };
            grouped.entry(actor).or_default().push(request);
        }
        // Preserve the oldest dirty actor ordering; object IDs are only tie breakers.
        let mut groups: Vec<_> = grouped.into_iter().collect();
        groups.sort_by_key(|(actor, requests)| {
            (
                requests
                    .iter()
                    .filter_map(|s| self.dirty.dirty_since(s.object_id))
                    .min()
                    .unwrap_or(now),
                *actor,
            )
        });
        let mut submitted = 0;
        for (actor, snapshots) in groups {
            if submitted >= budget || self.writes.contains_key(&actor) {
                for s in &snapshots {
                    self.dirty.failed(s).map_err(|e| e.to_string())?;
                }
                continue;
            }
            let p = self.players.get(&actor).ok_or("routine player missing")?;
            let since = self.origin
                + snapshots
                    .iter()
                    .filter_map(|s| self.dirty.dirty_since(s.object_id))
                    .min()
                    .unwrap_or(now);
            let mut participants: Vec<_> = self
                .items
                .iter()
                .filter(|(_, i)| i.owner == actor)
                .map(|(&id, _)| id)
                .collect();
            participants.push(actor);
            participants.sort_unstable();
            let batch = OwnedSaveBatch {
                snapshots,
                participants,
                leases: vec![p.lease],
            };
            match handle.try_owned_batch(&batch, since) {
                Ok(ticket) => {
                    self.writes.insert(
                        actor,
                        PendingWrite {
                            batch,
                            ticket: Some(ticket),
                            uncertain: false,
                        },
                    );
                    submitted += 1;
                }
                Err(e) => {
                    for s in &batch.snapshots {
                        self.dirty.failed(s).map_err(|e| e.to_string())?;
                    }
                    self.degraded = Some(e.to_string());
                }
            }
        }
        Ok(submitted)
    }
    pub fn poll_writes(&mut self, budget: usize) -> Result<usize, String> {
        let ids: Vec<_> = self
            .writes
            .range((
                std::ops::Bound::Excluded(self.poll_cursor),
                std::ops::Bound::Unbounded,
            ))
            .chain(self.writes.range(..=self.poll_cursor))
            .map(|(&id, _)| id)
            .take(budget)
            .collect();
        let mut completed = 0;
        for id in ids {
            self.poll_cursor = id;
            let pending = self.writes.get_mut(&id).expect("selected pending");
            let Some(ticket) = pending.ticket.as_mut() else {
                continue;
            };
            let report = match ticket.try_recv() {
                Ok(report) => report,
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => continue,
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    pending.ticket = None;
                    pending.uncertain = true;
                    self.degraded = Some("routine reply closed; reconciliation required".into());
                    continue;
                }
            };
            pending.ticket = None;
            match report.result {
                Ok(WriteOutcome::Routine(acks)) => {
                    if let Err(e) = self.accept_acks(id, &acks) {
                        self.writes
                            .get_mut(&id)
                            .expect("unresolved write")
                            .uncertain = true;
                        self.degraded = Some(e.clone());
                        return Err(e);
                    }
                    completed += 1;
                }
                Err(SaveFailure::Storage {
                    message,
                    uncertain: false,
                }) => {
                    let pending = self.writes.remove(&id).expect("selected pending");
                    for s in &pending.batch.snapshots {
                        self.dirty.failed(s).map_err(|e| e.to_string())?;
                    }
                    self.degraded = Some(message);
                }
                other => {
                    self.writes
                        .get_mut(&id)
                        .expect("selected pending")
                        .uncertain = true;
                    self.degraded = Some(format!("routine outcome unresolved: {other:?}"));
                }
            }
        }
        Ok(completed)
    }
    pub(super) fn accept_acks(&mut self, actor: u32, acks: &[SaveAck]) -> Result<(), String> {
        let pending = self
            .writes
            .get(&actor)
            .ok_or("routine acknowledgment without request")?;
        let actual: std::collections::BTreeSet<_> = acks
            .iter()
            .map(|a| (a.object_id, a.mutation_revision, a.persisted_version))
            .collect();
        let expected: std::collections::BTreeSet<_> = pending
            .batch
            .snapshots
            .iter()
            .map(|s| (s.object_id, s.mutation_revision, s.expected_version + 1))
            .collect();
        if actual.len() != acks.len() || actual != expected {
            return Err("routine acknowledgment mismatch".into());
        }
        let mut player = None;
        let mut items = Vec::new();
        for s in &pending.batch.snapshots {
            if s.object_id == actor {
                player = Some((
                    PlayerSaveV6::decode(&s.bytes).map_err(|e| e.to_string())?,
                    s.expected_version + 1,
                ));
            } else {
                items.push((
                    s.object_id,
                    bace_storage_codec::ItemSaveV5::decode(&s.bytes).map_err(|e| e.to_string())?,
                    s.expected_version + 1,
                ));
            }
        }
        // Exact all-row receipt preflight above prevents partial acknowledgment.
        for ack in acks {
            self.dirty.acknowledge(ack).map_err(|e| e.to_string())?;
        }
        let p = self
            .players
            .get_mut(&actor)
            .ok_or("routine actor missing")?;
        if let Some((saved, version)) = player {
            p.saved = saved;
            p.version = version;
        }
        if p.notice == p.captured_notice {
            p.first_dirty = None;
        }
        for (id, saved, version) in items {
            let i = self.items.get_mut(&id).ok_or("routine item missing")?;
            i.saved = saved.previous;
            i.source_destination = saved.source_destination;
            i.version = version;
        }
        self.writes.remove(&actor);
        Ok(())
    }
    /// Run on the bounded adapter I/O lane. Cancellation retains the uncertain
    /// request; another call rechecks under the same database writer locks.
    pub async fn resolve_one(&mut self, store: &bace_db_postgres::PgStore) -> Result<bool, String> {
        let Some((&id, pending)) = self.writes.iter().find(|(_, p)| p.uncertain) else {
            return Ok(false);
        };
        let result = store
            .resolve_owned_batch(&pending.batch)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(acks) = result {
            self.accept_acks(id, &acks)?;
        } else {
            let pending = self.writes.remove(&id).expect("retained uncertain write");
            for s in &pending.batch.snapshots {
                self.dirty.failed(s).map_err(|e| e.to_string())?;
            }
        }
        Ok(true)
    }
    pub fn retained_write_bytes(&self) -> usize {
        self.writes
            .values()
            .flat_map(|p| &p.batch.snapshots)
            .map(|s| s.bytes.len())
            .sum()
    }
    pub fn byte_limit(&self) -> usize {
        self.byte_limit
    }
}
