//! Valuable spellbook command snapshots and exact private-reply correlation.
use crate::saves::{SaveFailure, SaveHandle, SaveSubmitError, SaveTicket, WriteOutcome};
use bace_gameplay_api::staff::StaffSpellTicket;
use bace_persistence::{
    CharacterLease, OperationOutcome, OwnershipState, PlacementOperation, SaveAck, SaveSnapshot,
};
use bace_storage_codec::{PlayerSaveV6, SaveCodecError};
#[derive(Debug)]
pub enum StaffSpellResolution {
    Committed {
        ticket: StaffSpellTicket,
        ack: SaveAck,
    },
    Rejected {
        ticket: StaffSpellTicket,
        failure: SaveFailure,
    },
    Uncertain(String),
}
pub struct PendingStaffSpellSave {
    operation: PlacementOperation,
    ticket: StaffSpellTicket,
    receiver: Option<SaveTicket>,
    uncertain: bool,
    terminal: bool,
}
impl PendingStaffSpellSave {
    pub(crate) fn join_captured_inventory(
        &mut self,
        rows: Vec<SaveSnapshot>,
    ) -> Result<(), SaveCodecError> {
        let invalid = || SaveCodecError::Invalid("staff spell captured inventory identity");
        if self.receiver.is_some() || self.terminal || self.uncertain {
            return Err(invalid());
        }
        let mut snapshots = self.operation.snapshots.clone();
        let mut participants: std::collections::BTreeSet<_> =
            self.operation.participants.iter().copied().collect();
        for row in rows {
            if snapshots.iter().any(|s| s.object_id == row.object_id) {
                return Err(invalid());
            }
            let item = bace_storage_codec::ItemSaveV5::decode(&row.bytes)?;
            if item.entity.object_id != row.object_id
                || item.entity.mutation_revision != row.mutation_revision
                || row.expected_version <= 0
                || row.expected_version == i64::MAX
            {
                return Err(invalid());
            }
            participants.insert(row.object_id);
            snapshots.push(row);
        }
        if snapshots.len() > 1024
            || participants.len() > 1024
            || snapshots.iter().map(|s| s.bytes.len()).sum::<usize>() > 64 * 1024 * 1024
        {
            return Err(invalid());
        }
        self.operation.snapshots = snapshots;
        self.operation.participants = participants.into_iter().collect();
        Ok(())
    }
    pub fn operation(&self) -> &PlacementOperation {
        &self.operation
    }
    pub fn submit(&mut self, saves: &SaveHandle) -> Result<(), SaveSubmitError> {
        if self.terminal || self.receiver.is_some() {
            return Err(SaveSubmitError::Invalid);
        }
        self.receiver = Some(saves.try_placement(&self.operation)?);
        Ok(())
    }
    pub fn poll(&mut self) -> Option<StaffSpellResolution> {
        let report = match self.receiver.as_mut()?.try_recv() {
            Ok(report) => report,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.receiver = None;
                self.uncertain = true;
                return Some(StaffSpellResolution::Uncertain(
                    "staff spell save reply closed".into(),
                ));
            }
        };
        self.receiver = None;
        let s = &self.operation.snapshots[0];
        let expected = SaveAck {
            object_id: s.object_id,
            mutation_revision: s.mutation_revision,
            persisted_version: s.expected_version + 1,
        };
        let mut expected_all: Vec<_> = self
            .operation
            .snapshots
            .iter()
            .map(|s| SaveAck {
                object_id: s.object_id,
                mutation_revision: s.mutation_revision,
                persisted_version: s.expected_version + 1,
            })
            .collect();
        expected_all.sort_by_key(|s| s.object_id);
        let result = match report.result {
            Ok(WriteOutcome::Valuable(OperationOutcome::AlreadyCommitted)) => {
                StaffSpellResolution::Committed {
                    ticket: self.ticket.clone(),
                    ack: expected,
                }
            }
            Ok(WriteOutcome::Valuable(OperationOutcome::Committed(acks)))
                if {
                    let mut actual = acks.clone();
                    actual.sort_by_key(|s| s.object_id);
                    actual == expected_all
                } =>
            {
                StaffSpellResolution::Committed {
                    ticket: self.ticket.clone(),
                    ack: expected,
                }
            }
            Err(
                failure @ SaveFailure::Storage {
                    uncertain: false, ..
                },
            ) if !self.uncertain => StaffSpellResolution::Rejected {
                ticket: self.ticket.clone(),
                failure,
            },
            Err(failure) => StaffSpellResolution::Uncertain(failure.to_string()),
            Ok(_) => StaffSpellResolution::Uncertain("staff spell receipt mismatch".into()),
        };
        if matches!(result, StaffSpellResolution::Uncertain(_)) {
            self.uncertain = true;
        } else {
            self.terminal = true;
        }
        Some(result)
    }
}
pub fn freeze_staff_spell(
    operation_id: [u8; 16],
    ticket: &StaffSpellTicket,
    saved: &PlayerSaveV6,
    persisted_version: i64,
    lease: CharacterLease,
) -> Result<PendingStaffSpellSave, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("staff spell snapshot identity/revision");
    if operation_id == [0; 16]
        || ticket.operation == 0
        || ticket.spell == 0
        || ticket.spell > 65535
        || ticket.before.len() > 4096
        || ticket.after.len() > 4096
        || persisted_version < 1
        || persisted_version == i64::MAX
        || lease.state != OwnershipState::Online
        || lease.epoch <= 0
        || lease.character_id != ticket.context.actor.0
        || saved.player.entity.object_id != lease.character_id
        || saved.player.account_id != ticket.context.account.0
        || saved.player.entity.mutation_revision != ticket.before_revision
    {
        return Err(invalid());
    }
    let previous = saved
        .player
        .entity
        .state
        .properties
        .spell_book
        .iter()
        .map(|p| u32::try_from(p.id).map_err(|_| invalid()))
        .collect::<Result<Vec<_>, _>>()?;
    if previous != ticket.before {
        return Err(invalid());
    }
    let mut expected = ticket.before.clone();
    if ticket.learn {
        if !expected.contains(&ticket.spell) {
            expected.push(ticket.spell);
        }
    } else {
        expected.retain(|id| *id != ticket.spell);
    }
    if expected != ticket.after
        || ticket.after_revision
            != if ticket.before == ticket.after {
                ticket.before_revision
            } else {
                ticket.before_revision.checked_add(1).ok_or_else(invalid)?
            }
    {
        return Err(invalid());
    }
    let mut next = saved.clone();
    next.player.entity.mutation_revision = ticket.after_revision;
    next.player
        .entity
        .state
        .properties
        .spell_book
        .retain(|p| ticket.after.contains(&(p.id as u32)));
    if ticket.learn && !ticket.before.contains(&ticket.spell) {
        next.player
            .entity
            .state
            .properties
            .spell_book
            .push(bace_content::Property {
                id: ticket.spell as i32,
                value: 1.,
            });
    }
    let id = format!(
        "staff-spell:{}",
        operation_id
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let operation = PlacementOperation {
        operation_id: id,
        snapshots: vec![SaveSnapshot {
            object_id: lease.character_id,
            mutation_revision: ticket.after_revision,
            expected_version: persisted_version,
            bytes: next.encode()?,
        }],
        participants: vec![lease.character_id],
        leases: vec![lease],
        changes: vec![],
        storage_views: vec![],
    };
    Ok(PendingStaffSpellSave {
        operation,
        ticket: ticket.clone(),
        receiver: None,
        uncertain: false,
        terminal: false,
    })
}
