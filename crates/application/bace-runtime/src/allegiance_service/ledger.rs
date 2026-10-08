//! Indexed durable before-images. Only touched nodes are visited per operation.
use bace_persistence::{AllegianceOperation, AllegianceWrite, StoredAllegiance};
use std::collections::BTreeMap;

pub(super) struct Ledger {
    nodes: BTreeMap<u32, StoredAllegiance>,
    metadata: BTreeMap<u32, StoredAllegiance>,
    bytes: usize,
}
impl Ledger {
    pub(super) fn new(
        nodes: &[StoredAllegiance],
        metadata: &[StoredAllegiance],
    ) -> Result<Self, String> {
        let mut result = Self {
            nodes: BTreeMap::new(),
            metadata: BTreeMap::new(),
            bytes: 0,
        };
        for (rows, target) in [(nodes, &mut result.nodes), (metadata, &mut result.metadata)] {
            if rows.len() > 65536 {
                return Err("allegiance baseline count".into());
            }
            for row in rows {
                result.bytes = result
                    .bytes
                    .checked_add(row.bytes.len())
                    .ok_or("allegiance byte overflow")?;
                if row.character == 0
                    || row.persisted_version <= 0
                    || target.insert(row.character, row.clone()).is_some()
                {
                    return Err("allegiance baseline identity".into());
                }
            }
        }
        if result.bytes > 64 * 1024 * 1024 {
            return Err("allegiance baseline bytes".into());
        }
        Ok(result)
    }
    pub(super) fn before(
        &self,
        ticket: &bace_simulation::AllegianceTicket,
    ) -> (Vec<StoredAllegiance>, Vec<StoredAllegiance>) {
        let nodes = ticket
            .patch
            .nodes
            .iter()
            .filter_map(|(before, after)| before.as_ref().or(after.as_ref()))
            .filter_map(|n| self.nodes.get(&n.character.0).cloned())
            .collect();
        let metadata = ticket
            .patch
            .metadata
            .iter()
            .filter_map(|(before, after)| before.as_ref().or(after.as_ref()))
            .filter_map(|n| self.metadata.get(&n.monarch.0).cloned())
            .collect();
        (nodes, metadata)
    }
    pub(super) fn validate(&self, operation: &AllegianceOperation) -> Result<usize, String> {
        let mut bytes = self.bytes;
        for (writes, rows) in [
            (&operation.nodes, &self.nodes),
            (&operation.metadata, &self.metadata),
        ] {
            let mut count = rows.len();
            let mut ids = std::collections::BTreeSet::new();
            for write in writes {
                if !ids.insert(write.character)
                    || write.expected_version < 0
                    || write.expected_version == i64::MAX
                {
                    return Err("allegiance baseline write identity".into());
                }
                match rows.get(&write.character) {
                    Some(old)
                        if old.persisted_version == write.expected_version
                            && old.mutation_revision < write.mutation_revision =>
                    {
                        bytes -= old.bytes.len();
                        count -= 1;
                    }
                    None if write.expected_version == 0 => {}
                    _ => return Err("allegiance baseline CAS mismatch".into()),
                }
                if let Some(after) = &write.bytes {
                    bytes = bytes
                        .checked_add(after.len())
                        .ok_or("allegiance bytes overflow")?;
                    count += 1;
                }
            }
            if count > 65536 {
                return Err("allegiance baseline count".into());
            }
        }
        if bytes > 64 * 1024 * 1024 {
            return Err("allegiance baseline bytes".into());
        }
        Ok(bytes)
    }
    pub(super) fn adopt(&mut self, operation: &AllegianceOperation) -> Result<(), String> {
        let bytes = self.validate(operation)?;
        for (writes, rows) in [
            (&operation.nodes, &mut self.nodes),
            (&operation.metadata, &mut self.metadata),
        ] {
            for write in writes {
                adopt(rows, write);
            }
        }
        self.bytes = bytes;
        Ok(())
    }
}
fn adopt(rows: &mut BTreeMap<u32, StoredAllegiance>, write: &AllegianceWrite) {
    if let Some(bytes) = &write.bytes {
        rows.insert(
            write.character,
            StoredAllegiance {
                character: write.character,
                mutation_revision: write.mutation_revision,
                persisted_version: write.expected_version + 1,
                bytes: bytes.clone(),
            },
        );
    } else {
        rows.remove(&write.character);
    }
}
