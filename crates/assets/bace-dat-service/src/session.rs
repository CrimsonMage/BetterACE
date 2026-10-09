use crate::{DddCatalog, DddError, DddJob, DddLimits, DddStart, PreparedRecord, RecordMetadata};
use bace_wire::{
    DddBegin, DddControl, DddDatabase, DddInterrogationResponse, DddIteration, DddRequestData,
};
use std::{collections::VecDeque, sync::Arc};

pub struct DddSession {
    catalog: Arc<DddCatalog>,
    limits: DddLimits,
    enabled: bool,
    generation: u64,
    begun: bool,
    complete: bool,
    queue: VecDeque<RecordMetadata>,
    in_flight: Option<DddJob>,
    pending_bytes: u64,
}
impl DddSession {
    pub fn new(
        catalog: Arc<DddCatalog>,
        limits: DddLimits,
        enabled: bool,
        generation: u64,
    ) -> Result<Self, DddError> {
        limits.validate()?;
        Ok(Self {
            catalog,
            limits,
            enabled,
            generation,
            begun: false,
            complete: false,
            queue: VecDeque::new(),
            in_flight: None,
            pending_bytes: 0,
        })
    }
    pub fn interrogation(&self) -> Vec<u8> {
        DddControl::Interrogation {
            allow_highres: self
                .catalog
                .databases
                .iter()
                .any(|d| d.database == DddDatabase::HighRes),
        }
        .encode()
    }
    /// Atomically plans bounded work. No DAT reads/compression occur here.
    pub fn begin(&mut self, response: &DddInterrogationResponse) -> Result<DddStart, DddError> {
        if self.begun || self.complete {
            return Err(DddError::InvalidState);
        }
        if self.enabled && !self.catalog.transfer_ready {
            return Err(DddError::InvalidCatalog);
        }
        if response.with_keys.len() > 4 {
            return Err(DddError::InvalidIterations);
        }
        let mut iterations = Vec::new();
        let mut queue = VecDeque::new();
        let mut bytes = 0u64;
        let mut total_files = 0usize;
        for (database, index) in self
            .catalog
            .databases
            .iter()
            .zip(&self.catalog.iteration_indexes)
        {
            let (kind, id) = database.database.wire_identity();
            let sets: Vec<_> = response
                .with_keys
                .iter()
                .filter(|s| s.dat_file_type as u32 == kind && s.dat_file_id as u32 == id)
                .collect();
            if sets.len() > 1 {
                return Err(DddError::InvalidIterations);
            }
            let optional = matches!(database.database, DddDatabase::Cell | DddDatabase::HighRes);
            let Some(set) = sets.first() else {
                if optional {
                    continue;
                }
                return Err(DddError::MissingDatabase);
            };
            if optional && set.iterations == 0 {
                if !set.runs.is_empty() {
                    return Err(DddError::InvalidIterations);
                }
                continue;
            }
            let present = crate::iterations::present(set, database.iteration, self.limits)?;
            // Index-only catalogs are used when patching is disabled. Reject
            // mismatches without constructing an incomplete Begin plan.
            if !self.enabled {
                if present[1..].iter().any(|owned| !owned) {
                    return Err(DddError::Disabled);
                }
                continue;
            }
            for iteration in 1..=database.iteration {
                if present[iteration as usize] {
                    continue;
                }
                if iterations.len() >= self.limits.max_iterations as usize
                    || 12 + (iterations.len() + 1) * 20 + total_files * 4
                        > self.limits.max_message_bytes
                {
                    return Err(DddError::Capacity);
                }
                let mut files = Vec::new();
                for &offset in index.get(&iteration).into_iter().flatten() {
                    let record = &database.records[offset];
                    total_files += 1;
                    if total_files > self.limits.max_pending_records
                        || 12 + (iterations.len() + 1) * 20 + total_files * 4
                            > self.limits.max_message_bytes
                    {
                        return Err(DddError::Capacity);
                    }
                    files.push(record.object_id);
                    if database.database != DddDatabase::Cell {
                        bytes = bytes
                            .checked_add(u64::from(record.transfer_size))
                            .ok_or(DddError::Capacity)?;
                        if bytes > self.limits.max_transfer_bytes {
                            return Err(DddError::Capacity);
                        }
                        queue.push_back(*record);
                    }
                }
                iterations.push(DddIteration {
                    database: database.database,
                    iteration,
                    files,
                });
            }
        }
        if iterations.is_empty() {
            self.complete = true;
            return Ok(DddStart::UpToDate(DddControl::End.encode()));
        }
        if !self.enabled {
            return Err(DddError::Disabled);
        }
        let begin = DddBegin {
            total_file_size: bytes as u32,
            iterations,
        }
        .encode(
            self.limits.max_iterations as usize,
            self.limits.max_pending_records,
        )?;
        self.queue = queue;
        self.pending_bytes = bytes;
        self.begun = true;
        Ok(DddStart::Patch {
            begin,
            queued_records: self.queue.len(),
            transfer_bytes: bytes as u32,
        })
    }
    pub fn take_job(&mut self) -> Option<DddJob> {
        if self.in_flight.is_some() {
            return None;
        }
        let job = DddJob {
            generation: self.generation,
            record: *self.queue.front()?,
        };
        self.in_flight = Some(job);
        Some(job)
    }
    pub fn retry_job(&mut self, job: DddJob) -> Result<(), DddError> {
        if self.in_flight != Some(job) {
            return Err(DddError::StaleCompletion);
        }
        self.in_flight = None;
        Ok(())
    }
    /// Encodes before acknowledging preparation; mismatch or encoding failure
    /// retains the queued work. The caller must retain encoded output on backpressure.
    pub fn complete_job(
        &mut self,
        job: DddJob,
        record: &PreparedRecord,
    ) -> Result<Vec<u8>, DddError> {
        if self.in_flight != Some(job) {
            return Err(DddError::StaleCompletion);
        }
        if record.metadata() != job.record {
            return Err(DddError::PreparedRecordMismatch);
        }
        let bytes = record.encode(self.limits)?;
        self.queue.pop_front();
        self.pending_bytes -= u64::from(job.record.transfer_size);
        self.in_flight = None;
        Ok(bytes)
    }
    /// A client's completion is acknowledged only after all planned data has
    /// been prepared and handed to reliable output. It is not proof of local DAT writes.
    pub fn acknowledge_end(&mut self) -> Result<Option<Vec<u8>>, DddError> {
        if !self.begun {
            return Ok(None);
        }
        if !self.queue.is_empty() || self.in_flight.is_some() {
            return Err(DddError::InvalidState);
        }
        self.begun = false;
        self.complete = true;
        Ok(Some(DddControl::End.encode()))
    }
    /// Caller must additionally require a WorldConnected session. ACE permits
    /// on-demand requests only for LandBlock(1), LandBlockInfo(2), EnvCell(3).
    pub fn request(&mut self, request: DddRequestData) -> Result<(), DddError> {
        if !self.enabled {
            return Err(DddError::Disabled);
        }
        if !self.complete {
            return Err(DddError::InvalidState);
        }
        if !(1..=3).contains(&request.resource_type) {
            return Err(DddError::UnsupportedRequest);
        }
        let record = self
            .catalog
            .record(DddDatabase::Cell, request.object_id)
            .ok_or(DddError::NotFound)?;
        if record.resource_type != request.resource_type {
            return Err(DddError::UnsupportedRequest);
        }
        let mut records = Vec::with_capacity(2);
        if request.resource_type == 1
            && let Some(previous) = request.object_id.checked_sub(1)
            && let Some(info) = self.catalog.record(DddDatabase::Cell, previous)
        {
            records.push(info);
        }
        records.push(record);
        let added_bytes: u64 = records.iter().map(|r| u64::from(r.transfer_size)).sum();
        if self.queue.len().saturating_add(records.len()) > self.limits.max_pending_records
            || self.pending_bytes.saturating_add(added_bytes) > self.limits.max_transfer_bytes
        {
            return Err(DddError::Capacity);
        }
        self.pending_bytes += added_bytes;
        self.queue.extend(records);
        Ok(())
    }
    pub fn pending_records(&self) -> usize {
        self.queue.len()
    }
}
