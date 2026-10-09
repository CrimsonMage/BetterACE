//! Authenticated DAT negotiation and bounded transfer ownership. Archive I/O
//! stays on the cold asset worker; this adapter only routes retained packets.
use super::*;
use crate::dat_distribution::{DatPreparationWorker, DatSubmissionReason};
use bace_dat::DatArchive;
use bace_dat_service::{DddCatalog, DddError, DddLimits, DddSession, DddStart};
use bace_wire::opcode::GameMessageOpcode;
use bace_wire::{
    AccountControl, DddControl, DddDatabase, DddInterrogationResponse, DddRequestData, ServerName,
};
use std::collections::{BTreeMap, BTreeSet};

// GameRuntime correlations start at 0x4752; DDD owns this disjoint prefix so
// late network admissions can be discarded after disconnect without tombstones.
const DDD_CORRELATION_TAG: u64 = 0x4444_0000_0000_0000;
const DDD_CORRELATION_MASK: u64 = 0xffff_0000_0000_0000;

pub(super) struct DatRuntime {
    catalog: Arc<DddCatalog>,
    limits: DddLimits,
    enabled: bool,
    worker: Option<DatPreparationWorker>,
    sessions: BTreeMap<SessionKey, DddSession>,
    admissions: BTreeMap<u64, DatAdmission>,
    next_correlation: u64,
    patch_sessions: BTreeSet<SessionKey>,
    interrogations: BTreeMap<SessionKey, (Vec<u8>, bool)>,
    interrogation_admitted: BTreeSet<SessionKey>,
    ready: BTreeSet<SessionKey>,
}

struct DatAdmission {
    key: SessionKey,
    retry: Vec<(u16, Vec<u8>)>,
    queued: bool,
    kind: AdmissionKind,
    retry_after_ms: u64,
    retry_delay_ms: u64,
}

#[derive(Clone, Copy)]
enum AdmissionKind {
    Greeting,
    Interrogation,
    Begin,
    Data,
    End,
}

impl DatRuntime {
    /// Fingerprint admission is repeated against the files actually opened for
    /// distribution, closing the gap between world admission and DAT startup.
    pub(super) async fn prepare(bootstrap: &GameBootstrap) -> Result<Self, String> {
        let directory = bootstrap
            .config
            .dat_directory
            .clone()
            .ok_or("DAT distribution requires an approved DAT directory")?;
        let manifest = bootstrap.assets.clone();
        let enabled = bootstrap.config.dat_distribution.enabled;
        let limits = if enabled {
            DddLimits::stock_patch()
        } else {
            DddLimits::default()
        };
        eprintln!(
            "DDD startup: validating approved DAT catalog (patching {})",
            if enabled { "enabled" } else { "disabled" }
        );
        let (catalog, worker) = tokio::task::spawn_blocking(move || {
            crate::game_bootstrap::verify_assets(&manifest, &directory)?;
            let mut archives = vec![
                (
                    DddDatabase::Portal,
                    DatArchive::open(&manifest.portal).map_err(|e| e.to_string())?,
                ),
                (
                    DddDatabase::Language,
                    DatArchive::open(directory.join("client_local_English.dat"))
                        .map_err(|e| e.to_string())?,
                ),
                (
                    DddDatabase::Cell,
                    DatArchive::open(&manifest.cell).map_err(|e| e.to_string())?,
                ),
            ];
            let catalog = if enabled {
                DddCatalog::from_archives(&mut archives, limits)
            } else {
                DddCatalog::from_archive_indexes(&mut archives, limits)
            }
            .map_err(|e| e.to_string())?;
            let worker = if enabled {
                Some(DatPreparationWorker::spawn(archives, limits, 3).map_err(|e| e.to_string())?)
            } else {
                None
            };
            Ok::<_, String>((Arc::new(catalog), worker))
        })
        .await
        .map_err(|e| format!("DAT catalog worker: {e}"))??;
        eprintln!("DDD startup: DAT catalog ready");
        Ok(Self {
            catalog,
            limits,
            enabled,
            worker,
            sessions: BTreeMap::new(),
            admissions: BTreeMap::new(),
            next_correlation: 0,
            patch_sessions: BTreeSet::new(),
            interrogations: BTreeMap::new(),
            interrogation_admitted: BTreeSet::new(),
            ready: BTreeSet::new(),
        })
    }

    fn admission(
        &mut self,
        correlation: u64,
        key: SessionKey,
        retry: Vec<(u16, Vec<u8>)>,
        kind: AdmissionKind,
    ) -> Result<(), String> {
        if self.admissions.len() >= 4096 || self.admissions.contains_key(&correlation) {
            return Err("DDD reliable admission capacity or correlation conflict".into());
        }
        self.admissions.insert(
            correlation,
            DatAdmission {
                key,
                retry,
                queued: true,
                kind,
                retry_after_ms: 0,
                retry_delay_ms: 100,
            },
        );
        Ok(())
    }

    fn correlation(&mut self) -> Result<u64, String> {
        self.next_correlation = self
            .next_correlation
            .checked_add(1)
            .filter(|value| *value < (1_u64 << 48))
            .ok_or("DDD correlation exhausted")?;
        Ok(DDD_CORRELATION_TAG | self.next_correlation)
    }

    fn owns_correlation(correlation: u64) -> bool {
        correlation & DDD_CORRELATION_MASK == DDD_CORRELATION_TAG
    }

    pub(super) fn forget(&mut self, key: SessionKey) {
        self.sessions.remove(&key);
        self.patch_sessions.remove(&key);
        self.interrogations.remove(&key);
        self.interrogation_admitted.remove(&key);
        self.ready.remove(&key);
        self.admissions.retain(|_, admission| admission.key != key);
    }

    #[cfg(test)]
    pub(super) fn database_iteration(&self, database: DddDatabase) -> Option<u32> {
        self.catalog.database_iteration(database)
    }

    #[cfg(test)]
    pub(super) fn pending_admissions(&self) -> usize {
        self.admissions.len()
    }

    pub(super) fn pending(&self) -> bool {
        !self.sessions.is_empty()
            || !self.patch_sessions.is_empty()
            || !self.interrogations.is_empty()
            || !self.admissions.is_empty()
            || self
                .worker
                .as_ref()
                .is_some_and(|worker| worker.pending_jobs() != 0)
    }

    pub(super) fn ready_for_world(&self, key: SessionKey) -> bool {
        self.ready.contains(&key)
    }

    pub(super) fn shutdown(
        mut self,
    ) -> Result<(), (Self, Option<crate::dat_distribution::DatPreparationDrain>)> {
        if !self.sessions.is_empty()
            || !self.patch_sessions.is_empty()
            || !self.interrogations.is_empty()
            || !self.admissions.is_empty()
        {
            return Err((self, None));
        }
        if let Some(worker) = self.worker.take() {
            let drained = worker.shutdown();
            if drained.panicked || !drained.unrecovered_jobs.is_empty() {
                return Err((self, Some(drained)));
            }
        }
        Ok(())
    }
}

pub(super) enum DddIngress {
    Accepted,
    Blocked,
    Unsupported,
}

impl GameRuntime {
    pub(super) fn complete_roster_ddd(&mut self, key: SessionKey) -> Result<(), String> {
        let Some(dat) = self.dat.as_ref() else {
            // Synthetic unit fixtures without admitted DATs have no live DDD.
            return Ok(());
        };
        if dat.admissions.len() >= self.limits.sessions || dat.sessions.contains_key(&key) {
            return Err("DDD greeting capacity or duplicate session".into());
        }
        let ddd = DddSession::new(dat.catalog.clone(), dat.limits, dat.enabled, key.generation)
            .map_err(|e| e.to_string())?;
        let interrogation = ddd.interrogation();
        let name = ServerName {
            name: &self.bootstrap.config.world_name,
            current_connections: i32::try_from(self.sessions.len())
                .map_err(|_| "DDD session count overflow")?,
            max_connections: i32::try_from(self.limits.sessions)
                .map_err(|_| "DDD capacity overflow")?,
        }
        .encode()
        .map_err(|e| e.to_string())?;
        let correlation = self
            .dat
            .as_mut()
            .expect("checked DAT owner")
            .correlation()?;
        let messages = self
            .players
            .complete_roster_handshake(key, correlation, name)?;
        let dat = self.dat.as_mut().expect("checked DAT owner");
        dat.sessions.insert(key, ddd);
        dat.interrogations.insert(key, (interrogation, false));
        dat.admission(correlation, key, messages, AdmissionKind::Greeting)?;
        eprintln!("DDD session {}: roster and server name queued", key.id);
        Ok(())
    }

    pub(super) fn accept_ddd_admission(
        &mut self,
        key: SessionKey,
        correlation: u64,
        accepted: bool,
    ) -> bool {
        let Some(dat) = self.dat.as_mut() else {
            return false;
        };
        if !DatRuntime::owns_correlation(correlation) {
            return false;
        }
        if dat.admissions.get(&correlation).map(|a| a.key) != Some(key) {
            // A disconnected session already released its DDD output. The
            // network worker may still report its previously queued batch.
            return true;
        }
        if accepted {
            let admission = dat
                .admissions
                .remove(&correlation)
                .expect("matched DDD admission");
            match admission.kind {
                AdmissionKind::Greeting => {
                    if let Some((_, ready)) = dat.interrogations.get_mut(&key) {
                        *ready = true;
                    }
                }
                AdmissionKind::Interrogation => {
                    dat.interrogation_admitted.insert(key);
                }
                AdmissionKind::End => {
                    dat.ready.insert(key);
                }
                AdmissionKind::Begin | AdmissionKind::Data => {}
            }
        } else if let Some(admission) = dat.admissions.get_mut(&correlation) {
            admission.queued = false;
            let now = u64::try_from(self.last_elapsed.as_millis()).unwrap_or(u64::MAX);
            admission.retry_after_ms = now.saturating_add(admission.retry_delay_ms);
            admission.retry_delay_ms = admission.retry_delay_ms.saturating_mul(2).min(2_000);
        }
        true
    }

    pub(super) fn handle_ddd_message(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<DddIngress, String> {
        if message.bytes.len() < 4 {
            return Ok(DddIngress::Unsupported);
        }
        let opcode = u32::from_le_bytes(message.bytes[..4].try_into().expect("checked"));
        let response = GameMessageOpcode::DDD_InterrogationResponse.0;
        let end = GameMessageOpcode::DDD_EndDDD.0;
        let request = GameMessageOpcode::DDD_RequestDataMessage.0;
        if opcode != response && opcode != end && opcode != request {
            return Ok(DddIngress::Unsupported);
        }
        if message.queue != 5 {
            return self.reject_ddd(key, "Invalid DAT message queue");
        }
        let Some(dat) = self.dat.as_mut() else {
            return self.reject_ddd(key, "DAT negotiation unavailable");
        };
        let Some(ddd) = dat.sessions.get_mut(&key) else {
            return self.reject_ddd(key, "Unexpected DAT negotiation");
        };
        if self
            .sessions
            .get(&key)
            .is_none_or(|s| s.terminated || s.closing)
        {
            return Ok(DddIngress::Accepted);
        }
        if (opcode == response || opcode == end)
            && (self.network_output.len() >= self.limits.messages
                || dat.admissions.len() >= self.limits.sessions)
        {
            return Ok(DddIngress::Blocked);
        }
        if opcode == response {
            if !dat.interrogation_admitted.contains(&key) {
                return Ok(DddIngress::Blocked);
            }
            let parsed = DddInterrogationResponse::decode(
                &message.bytes,
                4,
                dat.limits.max_iterations,
                dat.limits.max_pending_records,
            );
            let result = parsed
                .map_err(DddError::from)
                .and_then(|response| ddd.begin(&response));
            match result {
                Ok(DddStart::UpToDate(bytes)) => {
                    self.publish_ddd(key, vec![(5, bytes)], AdmissionKind::End)?;
                    eprintln!("DDD session {}: client DATs match; end queued", key.id);
                }
                Ok(DddStart::Patch {
                    begin,
                    queued_records,
                    transfer_bytes,
                }) => {
                    if self
                        .dat
                        .as_ref()
                        .is_some_and(|dat| dat.patch_sessions.len() >= 2)
                    {
                        self.dat.as_mut().expect("DAT owner").forget(key);
                        return self.reject_ddd(key, "Server DAT patch slots are full");
                    }
                    self.dat
                        .as_mut()
                        .expect("DAT owner")
                        .patch_sessions
                        .insert(key);
                    self.publish_ddd(key, vec![(5, begin)], AdmissionKind::Begin)?;
                    eprintln!(
                        "DDD session {}: patch begun ({} records, {} bytes)",
                        key.id, queued_records, transfer_bytes
                    );
                }
                Err(DddError::Disabled) => {
                    return self.reject_ddd(
                        key,
                        "DAT update required; enable dat_distribution on the server",
                    );
                }
                Err(DddError::NewerClient) => {
                    return self.reject_ddd(key, "Client DAT version is newer than the server");
                }
                Err(error) => {
                    return self.reject_ddd(key, &format!("Invalid DAT negotiation: {error}"));
                }
            }
            return Ok(DddIngress::Accepted);
        }
        if opcode == end {
            if DddControl::decode_end(&message.bytes).is_err() {
                return self.reject_ddd(key, "Invalid DAT completion");
            }
            if dat
                .admissions
                .values()
                .any(|admission| admission.key == key)
            {
                return Ok(DddIngress::Blocked);
            }
            match ddd.acknowledge_end() {
                Ok(Some(bytes)) => {
                    self.dat
                        .as_mut()
                        .expect("DAT owner")
                        .patch_sessions
                        .remove(&key);
                    self.publish_ddd(key, vec![(5, bytes)], AdmissionKind::End)?;
                    eprintln!("DDD session {}: patch completed; end queued", key.id);
                }
                Ok(None) => return self.reject_ddd(key, "Unexpected DAT completion"),
                Err(_) => return Ok(DddIngress::Blocked),
            }
            return Ok(DddIngress::Accepted);
        }
        if self
            .sessions
            .get(&key)
            .and_then(|s| s.loading.as_ref())
            .is_none_or(|l| l.phase != lifecycle::Phase::Entered)
        {
            return self.reject_ddd(key, "DAT resource request before world entry");
        }
        if ddd.pending_records() != 0 || dat.admissions.values().any(|a| a.key == key) {
            return Ok(DddIngress::Blocked);
        }
        match DddRequestData::decode(&message.bytes)
            .map_err(DddError::from)
            .and_then(|request| ddd.request(request))
        {
            Ok(()) => Ok(DddIngress::Accepted),
            Err(error) => self.reject_ddd(key, &format!("DAT resource request failed: {error}")),
        }
    }

    pub(super) fn reject_ddd(
        &mut self,
        key: SessionKey,
        reason: &str,
    ) -> Result<DddIngress, String> {
        if self.network_output.len() >= self.limits.messages {
            return Ok(DddIngress::Blocked);
        }
        let bytes = AccountControl::Boot {
            reason: Some(reason),
        }
        .encode()
        .map_err(|e| e.to_string())?;
        self.network_output
            .push_back(NetworkCommand::TerminateAfterFlush {
                key,
                queue: 9,
                bytes,
            });
        eprintln!("DDD session {}: {}", key.id, reason);
        if let Some(session) = self.sessions.get_mut(&key) {
            session.closing = true;
        }
        Ok(DddIngress::Accepted)
    }

    fn publish_ddd(
        &mut self,
        key: SessionKey,
        messages: Vec<(u16, Vec<u8>)>,
        kind: AdmissionKind,
    ) -> Result<(), String> {
        if self.network_output.len() >= self.limits.messages {
            return Err("DDD output backpressure".into());
        }
        let dat = self.dat.as_mut().ok_or("DAT owner missing")?;
        let correlation = dat.correlation()?;
        dat.admission(correlation, key, messages.clone(), kind)?;
        self.network_output
            .push_back(NetworkCommand::SendReliableBatch {
                key,
                correlation,
                messages,
            });
        Ok(())
    }

    pub(super) fn poll_ddd(&mut self, elapsed: Duration) -> Result<(), String> {
        if self.dat.is_none() {
            return Ok(());
        }
        let now_ms = u64::try_from(elapsed.as_millis()).map_err(|_| "DDD clock overflow")?;
        for _ in 0..self.limits.work_per_poll {
            if self.network_output.len() >= self.limits.messages {
                break;
            }
            let retry = self.dat.as_ref().and_then(|dat| {
                dat.admissions.iter().find_map(|(correlation, admission)| {
                    (!admission.queued && now_ms >= admission.retry_after_ms)
                        .then(|| (*correlation, admission.key, admission.retry.clone()))
                })
            });
            let Some((correlation, key, messages)) = retry else {
                break;
            };
            if self
                .sessions
                .get(&key)
                .is_none_or(|s| s.terminated || s.closing)
            {
                self.dat.as_mut().expect("DAT owner").forget(key);
                continue;
            }
            self.dat
                .as_mut()
                .expect("DAT owner")
                .admissions
                .get_mut(&correlation)
                .expect("retained DDD admission")
                .queued = true;
            self.network_output
                .push_back(NetworkCommand::SendReliableBatch {
                    key,
                    correlation,
                    messages,
                });
        }
        for _ in 0..self.limits.work_per_poll {
            if self.network_output.len() >= self.limits.messages
                || self
                    .dat
                    .as_ref()
                    .is_some_and(|dat| dat.admissions.len() >= self.limits.sessions)
            {
                break;
            }
            let ready = self.dat.as_ref().and_then(|dat| {
                dat.interrogations
                    .iter()
                    .find_map(|(key, (_, ready))| ready.then_some(*key))
            });
            let Some(key) = ready else {
                break;
            };
            let (bytes, _) = self
                .dat
                .as_mut()
                .expect("DAT owner")
                .interrogations
                .remove(&key)
                .expect("ready interrogation");
            if let Err(error) =
                self.publish_ddd(key, vec![(5, bytes.clone())], AdmissionKind::Interrogation)
            {
                self.dat
                    .as_mut()
                    .expect("DAT owner")
                    .interrogations
                    .insert(key, (bytes, true));
                return Err(error);
            }
            eprintln!("DDD session {}: interrogation queued", key.id);
        }
        if self.dat.as_ref().is_none_or(|dat| dat.worker.is_none()) {
            return Ok(());
        }
        for _ in 0..self.limits.work_per_poll {
            if self.network_output.len() >= self.limits.messages
                || self
                    .dat
                    .as_ref()
                    .is_some_and(|dat| dat.admissions.len() >= self.limits.sessions)
            {
                break;
            }
            let completion = match self
                .dat
                .as_mut()
                .expect("DAT owner")
                .worker
                .as_mut()
                .expect("DAT worker")
                .try_receive()
            {
                Ok(Some(completion)) => completion,
                Ok(None) => break,
                Err(_) => return Err("DAT preparation worker disconnected".into()),
            };
            let dat = self.dat.as_mut().expect("DAT owner");
            let key = dat
                .sessions
                .keys()
                .find(|key| key.generation == completion.job.generation)
                .copied();
            let Some(key) = key else {
                continue;
            }; // Disconnected client cancelled its transfer.
            let ddd = dat.sessions.get_mut(&key).expect("matched DAT session");
            match completion.apply(ddd) {
                Ok(bytes) => self.publish_ddd(key, vec![(5, bytes)], AdmissionKind::Data)?,
                Err(error) => {
                    self.reject_ddd(key, &format!("DAT preparation failed: {error}"))?;
                    self.dat.as_mut().expect("DAT owner").forget(key);
                }
            }
        }
        let keys: Vec<_> = self
            .dat
            .as_ref()
            .expect("DAT owner")
            .sessions
            .keys()
            .copied()
            .take(self.limits.work_per_poll)
            .collect();
        for key in keys {
            if self
                .sessions
                .get(&key)
                .is_none_or(|s| s.terminated || s.closing)
                || self
                    .dat
                    .as_ref()
                    .is_some_and(|dat| dat.admissions.values().any(|a| a.key == key))
            {
                continue;
            }
            let Some(job) = self
                .dat
                .as_mut()
                .and_then(|dat| dat.sessions.get_mut(&key))
                .and_then(DddSession::take_job)
            else {
                continue;
            };
            let submitted = self
                .dat
                .as_mut()
                .and_then(|dat| dat.worker.as_mut())
                .expect("DAT worker")
                .try_submit(job);
            match submitted {
                Ok(()) => {}
                Err(error) => {
                    self.dat
                        .as_mut()
                        .and_then(|dat| dat.sessions.get_mut(&key))
                        .expect("DAT owner")
                        .retry_job(error.job)
                        .map_err(|e| e.to_string())?;
                    if error.reason != DatSubmissionReason::Full {
                        return Err("DAT preparation worker unavailable".into());
                    }
                    break;
                }
            }
        }
        Ok(())
    }
}
