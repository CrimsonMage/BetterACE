//! Explicit frozen NPC checkpoints. The runtime binds immutable definitions;
//! no gameplay struct is serialized and no whole conversation is collapsed.
mod service_completion;
pub use service_completion::{NpcServiceCompletionInput, freeze_service_completion};
mod terminal;
pub use terminal::{NpcCheckpointResolution, PendingNpcCheckpoint, freeze_terminal_checkpoint};
mod admission;
mod archive;
mod deletion_sources;
mod source_inventory;
mod source_state;
pub use deletion_sources::prepare_deletion_sources;
pub(crate) use source_inventory::{
    FrozenNpcSourceInventory, freeze_source_inventory, is_authored_static_shop,
};
mod source_stages;
pub use source_stages::{
    NpcArchiveStageInput, NpcDeleteSourceStage, NpcDeleteSourceStageInput, freeze_archive_stage,
    freeze_delete_source_stage,
};
mod handin;
pub use handin::{NpcHandInResolution, NpcHandInStageInput, PendingNpcHandIn, freeze_handin_stage};
mod effects;
mod inventory;
mod player_effects;
mod skill_reset;
mod spellbook;
mod teleport;
pub use skill_reset::{NpcSkillResetStageInput, freeze_skill_reset_stage};
pub use teleport::{NpcTeleportStage, NpcTeleportStageInput, freeze_teleport_stage};
mod stage;
mod training_credits;
pub use admission::{NpcServiceAdmissionInput, freeze_service_admission};
pub use inventory::{NpcInventoryStageInput, freeze_inventory_stage};
pub use spellbook::{NpcSpellbookStageInput, freeze_spellbook_stage};
pub use training_credits::{NpcTrainingCreditStageInput, freeze_training_credit_stage};
mod values;
use bace_simulation::NpcSourceCheckpoint;
use bace_storage_codec::{
    NpcExperienceSharingV2, NpcSharingPolicyV2, NpcWorkflowSaveV1, NpcWorkflowSaveV2,
    NpcWorkflowSaveV3, SaveCodecError, npc_workflow_v1::*,
};
pub use player_effects::freeze_player_effect;
pub use stage::{
    NpcExperienceAdmissionInput, NpcPlayerStageInput, NpcStageAdoption, NpcStageResolution,
    PendingNpcStage, freeze_experience_admission, freeze_player_stage,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NpcCheckpointBinding {
    /// Expected durable per-source head, independent of this invocation's stage.
    pub source_version: u64,
    /// Accepted original WCID, resolved from the pinned manifest, never a GUID.
    pub source_template: u32,
    pub invocation: [u8; 16],
    pub source: u32,
    pub program_hash: [u8; 32],
    pub content_generation: [u8; 32],
}
pub fn freeze_checkpoint(
    binding: NpcCheckpointBinding,
    stage: u64,
    source: NpcSourceCheckpoint,
) -> Result<NpcWorkflowSaveV3, SaveCodecError> {
    if source.source.0 != binding.source {
        return Err(SaveCodecError::Invalid("NPC source binding"));
    }
    let completed = source.vm.work.is_empty()
        && source.vm.pending.is_empty()
        && source.vm.detached.is_empty()
        && source.pending.is_empty();
    let archive = source.archive.map(archive::freeze).transpose()?;
    let live_properties = source
        .properties
        .map(source_state::freeze_properties)
        .transpose()?;
    let source_quests = source
        .source_quests
        .map(source_state::freeze_quests)
        .transpose()?;
    let experience_sharing = source
        .pending
        .iter()
        .filter_map(|p| {
            let bace_simulation::NpcEffect::QueuedExperience { share, .. } = p.proposal.effect
            else {
                return None;
            };
            Some(NpcExperienceSharingV2 {
                ticket: p.proposal.ticket,
                policy: match share {
                    bace_simulation::NpcExperienceSharing::None => NpcSharingPolicyV2::None,
                    bace_simulation::NpcExperienceSharing::Allegiance => {
                        NpcSharingPolicyV2::Allegiance
                    }
                    bace_simulation::NpcExperienceSharing::All => NpcSharingPolicyV2::All,
                },
            })
        })
        .collect();
    let previous = NpcWorkflowSaveV1 {
        invocation: binding.invocation,
        source: binding.source,
        program_hash: binding.program_hash,
        content_generation: binding.content_generation,
        logical_now: source.logical_now,
        event_id: source.event_id,
        key_version: source.key_version,
        random_position: source.random_position,
        active_operation: source.active_operation,
        invocations: source
            .invocations
            .into_iter()
            .map(|i| NpcInvocationSaveV1 {
                operation: i.operation,
                event_id: i.event_id,
                key_version: i.key_version,
                random_position: i.random_position,
            })
            .collect(),
        stage,
        completed,
        next_order: source.vm.order,
        remaining_instructions: source.vm.remaining,
        scheduled: source.vm.work.into_iter().map(freeze_row).collect(),
        pending: source
            .vm
            .pending
            .into_iter()
            .map(|p| NpcPendingRowV1 {
                ticket: p.ticket,
                row: freeze_row(p.row),
            })
            .collect(),
        detached: source.vm.detached,
        effects: source
            .pending
            .into_iter()
            .map(effects::freeze_pending)
            .collect::<Result<_, _>>()?,
    };
    let previous = NpcWorkflowSaveV2 {
        previous,
        experience_sharing,
    };
    let checkpoint = NpcWorkflowSaveV3 {
        inventory: source
            .inventory
            .map(source_state::freeze_inventory)
            .transpose()?,
        location: source
            .location
            .map(source_state::freeze_location)
            .transpose()?,
        live_properties,
        source_quests,
        previous,
        source_version: binding
            .source_version
            .checked_add(1)
            .ok_or(SaveCodecError::Invalid("NPC source head overflow"))?,
        source_template: binding.source_template,
        archive,
    };
    checkpoint.validate()?;
    Ok(checkpoint)
}
pub fn restore_checkpoint(
    binding: NpcCheckpointBinding,
    checkpoint: impl Into<NpcWorkflowSaveV3>,
) -> Result<NpcSourceCheckpoint, SaveCodecError> {
    let checkpoint = checkpoint.into();
    checkpoint.validate()?;
    if binding.source_template == 0
        || checkpoint.source_template != 0 && checkpoint.source_template != binding.source_template
    {
        return Err(SaveCodecError::Invalid("NPC source template binding"));
    }
    let location = checkpoint
        .location
        .map(source_state::thaw_location)
        .transpose()?;
    let archive = checkpoint.archive.map(archive::thaw).transpose()?;
    let properties = checkpoint
        .live_properties
        .map(source_state::thaw_properties)
        .transpose()?;
    let source_quests = checkpoint
        .source_quests
        .map(source_state::thaw_quests)
        .transpose()?;
    let checkpoint = checkpoint.previous;
    let sharing = checkpoint.experience_sharing;
    let checkpoint = checkpoint.previous;
    if checkpoint.invocation != binding.invocation
        || checkpoint.source != binding.source
        || checkpoint.program_hash != binding.program_hash
        || checkpoint.content_generation != binding.content_generation
    {
        return Err(SaveCodecError::Invalid("NPC checkpoint definition binding"));
    }
    Ok(NpcSourceCheckpoint {
        inventory: None,
        location,
        properties,
        source_quests,
        archive,
        source: bace_types::EntityId(checkpoint.source),
        logical_now: checkpoint.logical_now,
        event_id: checkpoint.event_id,
        key_version: checkpoint.key_version,
        random_position: checkpoint.random_position,
        active_operation: checkpoint.active_operation,
        invocations: checkpoint
            .invocations
            .into_iter()
            .map(|i| bace_simulation::NpcInvocationCheckpoint {
                operation: i.operation,
                event_id: i.event_id,
                key_version: i.key_version,
                random_position: i.random_position,
            })
            .collect(),
        vm: bace_emotes::NativeCheckpoint {
            order: checkpoint.next_order,
            remaining: checkpoint.remaining_instructions,
            work: checkpoint.scheduled.into_iter().map(thaw_row).collect(),
            pending: checkpoint
                .pending
                .into_iter()
                .map(|p| bace_emotes::NativePendingRow {
                    ticket: p.ticket,
                    row: thaw_row(p.row),
                })
                .collect(),
            detached: checkpoint.detached,
        },
        pending: checkpoint
            .effects
            .into_iter()
            .map(|effect| {
                let mut pending = effects::thaw_pending(effect)?;
                if let bace_simulation::NpcEffect::QueuedExperience { share, .. } =
                    &mut pending.proposal.effect
                {
                    let policy = sharing
                        .iter()
                        .find(|p| p.ticket == pending.proposal.ticket)
                        .ok_or(SaveCodecError::Invalid("missing NPC sharing policy"))?
                        .policy;
                    *share = match policy {
                        NpcSharingPolicyV2::None => bace_simulation::NpcExperienceSharing::None,
                        NpcSharingPolicyV2::Allegiance => {
                            bace_simulation::NpcExperienceSharing::Allegiance
                        }
                        NpcSharingPolicyV2::All => bace_simulation::NpcExperienceSharing::All,
                    };
                }
                Ok::<_, SaveCodecError>(pending)
            })
            .collect::<Result<_, _>>()?,
    })
}
fn freeze_row(value: bace_emotes::NativeScheduledRow) -> NpcScheduledRowV1 {
    NpcScheduledRowV1 {
        inline: value.inline,
        depth: value.depth,
        set: value.set,
        action: value.action,
        due: value.due,
        order: value.order,
        context: values::freeze_npc_context(value.context),
    }
}
fn thaw_row(value: NpcScheduledRowV1) -> bace_emotes::NativeScheduledRow {
    bace_emotes::NativeScheduledRow {
        inline: value.inline,
        depth: value.depth,
        set: value.set,
        action: value.action,
        due: value.due,
        order: value.order,
        context: values::thaw_npc_context(value.context),
    }
}
