//! Exact device and optional player cooldown snapshot in one valuable placement.
//! The caller retains the frozen operation through uncertain replies.
use crate::game_inventory::{FrozenInventoryItem, InventoryFreezeInput, freeze_inventory};
use bace_content::Position;
use bace_magic::EnchantmentEntry;
use bace_persistence::{CharacterLease, PlacementOperation, SaveSnapshot};
use bace_simulation::{
    InventoryReceipt, InventoryTicket, PlayerReadSnapshot, PlayerSnapshotOperation,
};
use bace_storage_codec::PlayerSaveV6;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetOperationId([u8; 16]);

impl PetOperationId {
    pub fn new(value: [u8; 16]) -> Result<Self, String> {
        (value != [0; 16])
            .then_some(Self(value))
            .ok_or("empty pet operation ID".into())
    }

    fn durable(self) -> String {
        use std::fmt::Write;
        let mut value = String::from("pet:");
        for byte in self.0 {
            write!(value, "{byte:02x}").expect("String write");
        }
        value
    }
}

pub struct PetSaveInput<'a> {
    pub id: PetOperationId,
    pub ticket: &'a InventoryTicket,
    pub snapshot: &'a PlayerReadSnapshot,
    pub player_baseline: &'a PlayerSaveV6,
    pub player_version: i64,
    pub lease: CharacterLease,
    pub items: &'a [FrozenInventoryItem],
    pub captured_items: &'a [SaveSnapshot],
    pub captured_unix_millis: u64,
    /// Summon may add a cooldown. A release never changes the registry.
    pub registry_after: Option<(u64, &'a [EnchantmentEntry])>,
    pub summon: bool,
}

pub struct FrozenPetSave {
    pub operation: PlacementOperation,
    pub receipt: InventoryReceipt,
}

pub fn freeze_pet(input: PetSaveInput<'_>) -> Result<FrozenPetSave, String> {
    let actor = input.snapshot.binding().actor;
    let ticket = input.ticket;
    let before_revision = input.snapshot.character().progression().revision();
    if ticket.operation == 0
        || ticket.actor != actor
        || input.lease.character_id != actor.0
        || input.player_version <= 0
        || input.snapshot.operation()
            != Some((
                PlayerSnapshotOperation::Pet(ticket.operation),
                before_revision,
            ))
        || ticket.proposal.changes.len() != 1
        || ticket.proposal.changes[0]
            .before
            .as_ref()
            .is_none_or(|before| {
                before.id != ticket.proposal.changes[0].after.id
                    || before.active_pet == input.summon
                    || ticket.proposal.changes[0].after.active_pet != input.summon
            })
    {
        return Err("pet save identity or device transition".into());
    }
    let mut player = crate::player_saves::freeze_player_operation_baseline(
        input.player_baseline,
        input.snapshot,
        PlayerSnapshotOperation::Pet(ticket.operation),
        before_revision,
        input.captured_unix_millis,
    )
    .map_err(|e| e.to_string())?;
    let current_registry = input
        .snapshot
        .enchantments()
        .ok_or("pet player registry missing")?;
    let changed_registry = if let Some((revision, after)) = input.registry_after {
        if revision == current_registry.revision() && after == current_registry.entries() {
            false
        } else if revision
            == current_registry
                .revision()
                .checked_add(1)
                .ok_or("pet registry overflow")?
            && after != current_registry.entries()
        {
            player.enchantments = after
                .iter()
                .map(crate::enchantment_saves::freeze_enchantment)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            player.player.entity.mutation_revision = player
                .player
                .entity
                .mutation_revision
                .checked_add(1)
                .ok_or("pet player revision overflow")?;
            true
        } else {
            return Err("pet cooldown registry revision or before image".into());
        }
    } else {
        false
    };
    if !input.summon && changed_registry {
        return Err("pet release cannot create cooldown".into());
    }
    player.validate().map_err(|e| e.to_string())?;
    let mut other = input.captured_items.to_vec();
    let changed_item = ticket.proposal.changes[0].after.id.0;
    other.retain(|row| row.object_id != changed_item);
    if changed_registry || player != *input.player_baseline {
        other.push(SaveSnapshot {
            object_id: actor.0,
            mutation_revision: player.player.entity.mutation_revision,
            expected_version: input.player_version,
            bytes: player.encode().map_err(|e| e.to_string())?,
        });
    }
    let mut unique = BTreeSet::new();
    if other.iter().any(|row| !unique.insert(row.object_id)) {
        return Err("duplicate pet snapshot".into());
    }
    let operation_id = input.id.durable();
    let operation = freeze_inventory(InventoryFreezeInput {
        operation_id: &operation_id,
        proposal: &ticket.proposal,
        items: input.items,
        other_snapshots: &other,
        leases: &[input.lease],
        storage_views: &[],
        admitted_positions: &BTreeMap::<u32, Position>::new(),
    })
    .map_err(|e| e.to_string())?;
    let receipt = InventoryReceipt {
        operation: ticket.operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|change| (change.after.id, change.after.revision))
            .collect(),
    };
    Ok(FrozenPetSave { operation, receipt })
}
