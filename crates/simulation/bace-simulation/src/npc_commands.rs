//! Trusted bounded NPC adapter commands. Correlations identify delivery, while
//! exact owner proposals and durable receipts authorize gameplay adoption.
use crate::{
    InventoryReceipt, NpcInventoryTicket, NpcProposal, NpcSourceCheckpoint, NpcSpellbookTicket,
    NpcTrainingCreditTicket,
};
use bace_gameplay_api::{
    CastChange, DoorChange, NpcCompletion, NpcDestination, NpcFailure, ServerCastOutcome,
};
use bace_types::EntityId;
use std::sync::Arc;
#[derive(Clone, Debug)]
pub struct NpcServiceCommand {
    pub correlation: u64,
    pub action: NpcServiceAction,
}
#[derive(Clone, Debug)]
pub enum NpcServiceAction {
    InspectMotion {
        proposal: NpcProposal,
    },
    BeginPreparedMotion {
        proposal: NpcProposal,
        actor: EntityId,
        epoch: u16,
        revision: u64,
        before: bace_motion::SourceMotionState,
        chain: Arc<bace_motion::PreparedMotionChain>,
    },
    BeginGive {
        proposal: NpcProposal,
    },
    PreviewServiceAdmissionNow {
        proposal: NpcProposal,
        post_delay: f64,
    },
    InspectCast {
        proposal: NpcProposal,
    },
    RegisterCast {
        proposal: NpcProposal,
        epoch: u16,
        property_revision: u64,
        motion: Option<bace_motion::SourceMotionState>,
        source: Arc<bace_content::WeenieV1>,
        definition: crate::PreparedMagicDefinition,
        shapes: Vec<(u32, Arc<bace_physics::CollisionShape>)>,
    },
    PrepareInventoryBatch {
        proposal: NpcProposal,
        items: Vec<bace_inventory::InventoryItem>,
        containers: Vec<bace_inventory::InventoryContainer>,
    },
    Use {
        context: bace_gameplay_api::ActionContext,
        source: EntityId,
        event: [u8; 16],
        operation: u64,
    },
    FreezeIdle {
        source: EntityId,
        operation: u64,
    },
    CommitIdle {
        source: EntityId,
        operation: u64,
    },
    ReleaseIdle {
        source: EntityId,
        operation: u64,
    },
    ReleaseJournal {
        proposal: NpcProposal,
    },
    PreviewDeletionNow {
        ticket: crate::NpcDeleteSourceTicket,
    },
    PreviewExperienceAdmissionNow {
        proposal: NpcProposal,
    },
    PreviewOwnerCompletion {
        proposal: NpcProposal,
    },
    BindAdmitted {
        actor: EntityId,
        identity: crate::npc::NpcScriptIdentity,
    },
    RestoreObjectRegistry {
        source: EntityId,
        revision: u64,
        entries: Vec<bace_magic::EnchantmentEntry>,
    },
    RestoreIdleSourceState {
        checkpoint: crate::NpcSourceCheckpoint,
    },
    RegisterRecovery {
        actor: EntityId,
        program: Arc<bace_emotes::NativeProgram>,
        use_radius: f32,
        properties: Option<bace_entity::EntityProperties>,
    },
    Register {
        actor: EntityId,
        program: Arc<bace_emotes::NativeProgram>,
        use_radius: f32,
        properties: Option<bace_entity::EntityProperties>,
    },
    Start {
        source: EntityId,
        target: Option<EntityId>,
        trigger: bace_emotes::NativeTrigger,
        event: [u8; 16],
        operation: u64,
        check_range: bool,
    },
    PrepareDeletion {
        proposal: NpcProposal,
    },
    PreviewDeletion {
        ticket: crate::NpcDeleteSourceTicket,
        logical_now: f64,
    },
    CommitRetirement {
        ticket: crate::NpcDeleteSourceTicket,
        receipt: InventoryReceipt,
    },
    CommitTransientDeletion {
        ticket: crate::NpcDeleteSourceTicket,
    },
    RejectDeletion {
        ticket: crate::NpcDeleteSourceTicket,
    },
    RegisterArchive {
        checkpoint: NpcSourceCheckpoint,
        program: Arc<bace_emotes::NativeProgram>,
        use_radius: f32,
    },
    ReleaseArchive {
        source: EntityId,
    },
    KillSelf {
        proposal: NpcProposal,
    },
    PrepareTeleport {
        proposal: NpcProposal,
    },
    CommitTeleport {
        ticket: crate::NpcPortalTicket,
        receipt: crate::PortalServiceReceipt,
    },
    RejectTeleport {
        ticket: crate::NpcPortalTicket,
    },
    ResetHome {
        proposal: NpcProposal,
    },
    PrepareSkillReset {
        proposal: NpcProposal,
    },
    CommitSkillReset {
        ticket: crate::NpcSkillResetTicket,
    },
    RejectSkillReset {
        ticket: crate::NpcSkillResetTicket,
    },
    PrepareHandIn {
        request: crate::NpcHandInRequest,
    },
    CommitHandIn {
        ticket: crate::NpcHandInTicket,
        receipt: InventoryReceipt,
    },
    RejectHandIn {
        ticket: crate::NpcHandInTicket,
    },
    PrepareInventory {
        proposal: NpcProposal,
        item: Option<bace_inventory::InventoryItem>,
    },
    CommitInventory {
        ticket: NpcInventoryTicket,
        receipt: InventoryReceipt,
    },
    RejectInventory {
        ticket: NpcInventoryTicket,
    },
    PrepareSpellbook {
        proposal: NpcProposal,
    },
    CommitSpellbook {
        ticket: NpcSpellbookTicket,
    },
    RejectSpellbook {
        ticket: NpcSpellbookTicket,
    },
    PrepareTrainingCredits {
        proposal: NpcProposal,
    },
    CommitTrainingCredits {
        ticket: NpcTrainingCreditTicket,
    },
    RejectTrainingCredits {
        ticket: NpcTrainingCreditTicket,
    },
    BeginCast {
        proposal: NpcProposal,
    },
    PollCast {
        proposal: NpcProposal,
    },
    CommitCast {
        proposal: NpcProposal,
        outcome: ServerCastOutcome,
    },
    BeginMotion {
        proposal: NpcProposal,
        chain: Arc<bace_motion::PreparedMotionChain>,
    },
    PollMotion {
        proposal: NpcProposal,
    },
    CommitMotion {
        proposal: NpcProposal,
        event: bace_world::WorldMotionEvent,
    },
    BeginMovement {
        proposal: NpcProposal,
        home: Option<NpcDestination>,
    },
    PollMovement {
        proposal: NpcProposal,
    },
    CancelMovement {
        proposal: NpcProposal,
    },
    Door {
        proposal: NpcProposal,
    },
    Generate {
        proposal: NpcProposal,
    },
    Signal {
        proposal: NpcProposal,
    },
    PreviewCompletion {
        proposal: NpcProposal,
        completion: NpcCompletion,
        logical_now: f64,
    },
    PreviewServiceAdmission {
        proposal: NpcProposal,
        logical_now: f64,
        post_delay: f64,
    },
    AdmitService {
        proposal: NpcProposal,
        post_delay: f64,
    },
    PreviewExperienceAdmission {
        proposal: NpcProposal,
        logical_now: f64,
    },
    AdmitExperience {
        proposal: NpcProposal,
    },
    CommitPlayerEffect {
        proposal: NpcProposal,
    },
    Checkpoint {
        source: EntityId,
    },
    Restore {
        checkpoint: NpcSourceCheckpoint,
    },
    RecoveryReady {
        source: EntityId,
    },
}
#[derive(Clone, Debug)]
pub enum NpcServiceResult {
    MotionSource {
        actor: EntityId,
        epoch: u16,
        revision: u64,
        before: bace_motion::SourceMotionState,
        scale: f64,
    },
    GiveStarted {
        detached: bool,
        turning: bool,
    },
    CastSource {
        epoch: u16,
        property_revision: u64,
        motion: Option<bace_motion::SourceMotionState>,
        scale: f64,
        qualities: bace_entity::EntityProperties,
    },
    Started(bool),
    Deletion(crate::NpcDeleteSourceTicket),
    Killed {
        damage: u32,
    },
    Teleport(crate::NpcPortalTicket),
    Home(NpcDestination),
    SkillReset(crate::NpcSkillResetTicket),
    HandIn(crate::NpcHandInTicket),
    Signal {
        listeners: usize,
    },
    Applied,
    Inventory(NpcInventoryTicket),
    Spellbook(NpcSpellbookTicket),
    TrainingCredits(NpcTrainingCreditTicket),
    CastStarted(CastChange),
    CastProgress(Option<ServerCastOutcome>),
    MotionStarted(bace_motion::MotionToken),
    MotionProgress(Option<bace_world::WorldMotionEvent>),
    MovementProgress(bool),
    Door(DoorChange),
    Checkpoint(NpcSourceCheckpoint),
}
#[derive(Clone, Debug)]
pub struct NpcServiceOutcome {
    pub correlation: u64,
    pub result: Result<NpcServiceResult, NpcFailure>,
}
impl NpcServiceCommand {
    pub fn valid_bounds(&self) -> bool {
        if self.correlation == 0 {
            return false;
        }
        match &self.action {
            NpcServiceAction::RestoreObjectRegistry {
                source, entries, ..
            } => source.0 != 0 && entries.len() <= 512,
            NpcServiceAction::RegisterRecovery {
                actor,
                use_radius,
                properties,
                ..
            }
            | NpcServiceAction::Register {
                actor,
                use_radius,
                properties,
                ..
            } => {
                actor.0 != 0
                    && use_radius.is_finite()
                    && properties
                        .as_ref()
                        .is_none_or(|p| p.retained_bytes().is_some_and(|n| n <= 2 * 1024 * 1024))
            }
            NpcServiceAction::Start {
                source,
                target,
                trigger,
                event,
                operation,
                ..
            } => {
                source.0 != 0
                    && target.is_none_or(|t| t.0 != 0)
                    && *event != [0; 16]
                    && *operation != 0
                    && trigger.quest.as_ref().is_none_or(|s| s.len() <= 4096)
            }
            NpcServiceAction::PrepareHandIn { request } => {
                request.count > 0 && request.event != [0; 16] && request.operation > 0
            }
            NpcServiceAction::CommitHandIn { ticket, receipt } => {
                ticket.inventory.proposal.changes.len() <= 1024
                    && ticket.inventory.proposal.participants.len() <= 1024
                    && ticket.checkpoint.pending.is_empty()
                    && ticket.checkpoint.vm.work.len() <= 76
                    && receipt.revisions.len() <= 1024
            }
            NpcServiceAction::RejectHandIn { ticket } => {
                ticket.inventory.proposal.changes.len() <= 1024
                    && ticket.inventory.proposal.participants.len() <= 1024
                    && ticket.checkpoint.pending.is_empty()
                    && ticket.checkpoint.vm.work.len() <= 76
            }
            NpcServiceAction::CommitInventory { ticket, receipt } => {
                ticket.inventory.proposal.changes.len() <= 1024
                    && ticket.inventory.proposal.participants.len() <= 1024
                    && receipt.revisions.len() <= 1024
            }
            NpcServiceAction::RejectInventory { ticket } => {
                ticket.inventory.proposal.changes.len() <= 1024
                    && ticket.inventory.proposal.participants.len() <= 1024
            }
            NpcServiceAction::CommitSpellbook { ticket }
            | NpcServiceAction::RejectSpellbook { ticket } => {
                ticket.before.len() <= 4096 && ticket.after.len() <= 4096
            }
            NpcServiceAction::Restore { checkpoint }
            | NpcServiceAction::RestoreIdleSourceState { checkpoint } => {
                checkpoint
                    .properties
                    .as_ref()
                    .is_none_or(|p| p.retained_bytes().is_some_and(|n| n <= 2 * 1024 * 1024))
                    && checkpoint.source_quests.as_ref().is_none_or(|(_, rows)| {
                        rows.len() <= 4096 && rows.iter().all(|(name, _)| name.len() <= 256)
                    })
                    && checkpoint.pending.len() <= 4096
                    && checkpoint.invocations.len() <= 4096
                    && checkpoint.vm.pending.len() <= 4096
                    && checkpoint.vm.work.len() <= 4096
                    && checkpoint.vm.detached.len() <= 4096
            }
            NpcServiceAction::PreviewCompletion { logical_now, .. }
            | NpcServiceAction::PreviewExperienceAdmission { logical_now, .. } => {
                logical_now.is_finite() && *logical_now >= 0.0
            }
            NpcServiceAction::PreviewServiceAdmission {
                logical_now,
                post_delay,
                ..
            } => {
                logical_now.is_finite()
                    && *logical_now >= 0.0
                    && post_delay.is_finite()
                    && *post_delay >= 0.0
            }
            NpcServiceAction::AdmitService { post_delay, .. } => {
                post_delay.is_finite() && *post_delay >= 0.0
            }
            _ => true,
        }
    }
}
