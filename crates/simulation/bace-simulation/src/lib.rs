//! Tick phases and gameplay-system orchestration.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

#[cfg(test)]
extern crate self as bace_simulation;

mod characters;
mod combat;
mod doors;
mod kernel;
mod pve;
mod scenario;

pub use bace_economy::{VendorBuyLine, VendorBuyQuote, VendorBuyRequest};
pub use characters::CharacterRegistrationError;
pub use combat::{CombatEvent, DeathBlow};
pub use kernel::{Command, Kernel, SimulationError};
pub use kernel::{VendorBuyError, VendorBuyReceipt, VendorBuyReservation, VendorBuySource};
pub use player_death::{
    CorpseAccessCommand, CorpseAccessInspection, CorpseAccessOutcome, CorpseConsentCommand,
    CorpseConsentError, CorpseConsentOutcome,
};
pub use pve::{
    AppraisalCreatureKind, AppraisalResponseKind, AppraisalRoll, AppraisalSelectionError,
    AppraisalSelectionResult, AppraisalSourceTables, AppraisalTargetCheck, AppraisalWake,
    DeathProposal, LootDrop, NpcBlueprint, NpcLootEntry, PveError, PveEvent,
    evaluate_appraisal_roll, select_appraisal,
};
pub use scenario::synthetic_scenario;

pub use doors::{DoorEvent, PreparedDoor, PreparedDoorAnimation, PreparedDoorHook};

mod npc;
pub use kernel::PreparedVitalInputs;
pub use magic::{PreparedActorMagicProgram, PreparedMagicAssetBatch, PreparedMagicDefinition};
pub use npc::NpcDeleteSourceTicket;
pub use npc::NpcPortalTicket;
pub use npc::NpcSkillResetTicket;
pub use npc::NpcTrainingCreditTicket;
pub use npc::{
    NpcEffect, NpcExperienceSharing, NpcInventoryTicket, NpcNotification, NpcProposal,
    NpcQueuedExperiencePhase, NpcSpellbookTicket,
};
pub use npc::{NpcHandInRequest, NpcHandInTicket};
pub use npc::{NpcScriptIdentity, PreparedNpcScriptSource};
pub use npc::{NpcSourceArchive, NpcSourceInventorySnapshot, NpcSourceLocation};
pub use player_admission::PreparedItemSpellTarget;
mod npc_commands;
pub use npc_commands::{NpcServiceAction, NpcServiceCommand, NpcServiceOutcome, NpcServiceResult};

mod magic;
pub use magic::{
    MagicCaster, MagicEvent, MagicResourceCommit, PreparedCastGesture, PreparedMagicSpell,
};
mod inventory;
mod inventory_commands;
mod inventory_equipment;
pub use inventory::{InventoryReceipt, InventoryTicket};
pub use inventory_commands::{
    InventoryCommand, InventoryCommandKind, InventoryDecision, InventoryInspection,
    InventoryLivePrepared, InventoryMotion, InventoryOperation, InventoryOutcome,
    InventoryPreparedRequest,
};
pub use inventory_equipment::{
    EquipmentVitalChange, PreparedEquipmentPhysical, PreparedEquipmentRequest,
};

pub use characters::OwnedCharacterState;

pub use pve::{NativeDeathLoot, NativeLootPolicy};

pub use characters::OwnedUiState;

mod housing;
pub use housing::{HousingReceipt, HousingRegistration, HousingTicket};

pub use characters::AttributeTransferTicket;
pub use characters::{SkillActionError, SkillIntent, SkillTicket};

pub use characters::{SkillOutcome, SkillStage, UiOutcome};

pub use characters::OwnedPlayerState;

mod attribute_transfer;
mod crafting;
pub use attribute_transfer::{
    AttributeTransferCommand, AttributeTransferConfirmation, AttributeTransferDeviceError,
    AttributeTransferDeviceTicket, AttributeTransferOutcome, AttributeTransferResult,
    PreparedAttributeTransfer,
};
pub use crafting::{CraftingDecision, CraftingProficiency, CraftingTicket, SalvageAdmission};
mod skill_devices;

pub use skill_devices::{
    PreparedSkillDevice, SkillDeviceConfirmation, SkillDeviceCooldown, SkillDeviceError,
    SkillDeviceTicket,
};

mod device_commands;
pub use device_commands::{SkillDeviceCommand, SkillDeviceOutcome, SkillDeviceResult};

pub use kernel::{HousingPaymentBinding, HousingPaymentError};

pub use combat::{
    DirtyFightingImpact, PreparedCharacterSkillInputs, PreparedShield, SkillRefreshError,
};

pub use combat::PreparedAttributeModifier;
pub use crafting::{
    CraftingCommand, CraftingCommandKind, CraftingOutcome, CraftingResult, SalvageCommandInput,
    TinkerCommandInput,
};

pub use combat::PhysicalCombatEvent;

pub use npc::{NpcPendingCheckpoint, NpcSourceCheckpoint};

mod pets;
pub use bace_ai::{PetUseError, PetUseRequirements, PetUser};
pub use pets::{PetError, PetEvent, PreparedCombatPet, PreparedPassivePet};
mod pet_commands;
pub use pet_commands::{PetAction, PetCommand, PetDecision, PetOutcome};

pub use magic::PeriodicDefenseProfile;
pub use npc::NpcInvocationCheckpoint;

pub use npc::NpcAggregateFence;

pub use kernel::{
    PortalAcceptedView, PortalServiceEffect, PortalServiceEvent, PortalServiceOrigin,
    PortalServiceReceipt, PortalServiceTicket,
};

mod player_world;
pub use player_world::{PlayerWorldSaveMarker, PlayerWorldSnapshot};

pub use pve::{GeneratedNpcDeath, GeneratedNpcOrigin, PreparedNpcGeometry};
mod generators;
pub use generators::{
    GeneratedNpcTemplate, GeneratorControl, GeneratorHostRequest, GeneratorServiceError,
    GeneratorWorldEvent, PreparedGeneratorRegion, PreparedGeneratorRoot, PreparedNpcLoadout,
    PreparedNpcMissileTiming,
};

mod generator_commands;
pub use generator_commands::{
    GeneratorAction, GeneratorCommand, GeneratorCommandOutcome, GeneratorItemAdmission,
};
pub use pve::AceCreatureLootPolicy;

pub use kernel::GeneratedRetirementTicket;

pub use kernel::PreparedGeneratorEnchantment;

pub mod region_residency;
pub use region_residency::*;

mod staff;
pub use staff::StaffState;

mod recalls;
pub use recalls::{
    PreparedBindingObject, PreparedRecallHouse, PreparedRecallLocations, RecallCommand, RecallEvent,
};
mod allegiances;
mod fellowships;
mod social;

pub use allegiances::AllegianceTicket;
mod player_death;
pub use player_death::{
    CorpseAccessDecision, CorpseAccessDenial, CorpseAccessError, CorpseAccessProfile,
    DeathCoinSource, DeathDestroyedReceipt, DeathDropOrigin, DeathDropReceipt,
    DeathInventoryTranscript, NoCorpseDescendant, OlthoiDeathKind, PlayerDeathAnnouncement,
    PlayerDeathError, PlayerDeathEvent, PlayerDeathReceipt, PlayerDeathState, PlayerDeathTicket,
    PlayerNoCorpsePlan, PlayerVitaeRecovery, PreparedOlthoiDeath, PreparedPlayerDeath,
    PreparedPlayerNoCorpse,
};

pub use kernel::{PreparedStackDrop, StackWorldPlacement};

mod social_controls;
pub use social_controls::{SocialControl, SocialControlAction, SocialControlOutcome};

pub use player_death::PlayerDeathCommand;

mod item_experience;
pub use item_experience::{
    ItemExperienceEvent, ItemExperienceRegistryChange, ItemExperienceReward,
    PreparedItemExperience, PreparedItemSet,
};

mod pve_commands;
pub use pve_commands::{PveServiceAction, PveServiceCommand, PveServiceOutcome};

mod player_snapshot;
pub use characters::CharacterReadSnapshot;
pub use player_snapshot::{
    PlayerReadSnapshot, PlayerSnapshotOperation, PlayerSnapshotOutcome, PlayerSnapshotRequest,
};

mod region_unload;
pub use region_unload::{RegionUnloadItem, RegionUnloadReceipt, RegionUnloadTicket};

mod player_admission;
pub use player_admission::{
    PlayerAdmissionError, PlayerAdmissionOutcome, PlayerAdmissionRequest, PlayerEnteredOutcome,
    PlayerEnteredRequest, PreparedPlayerAdmission,
};

mod corpse_expiry;
pub use corpse_expiry::{
    CorpseDecayChange, CorpseExpiryEvent, CorpseExpiryPhase, CorpseExpiryTicket, CorpseSpillIntent,
    PreparedCorpseSpill,
};

pub use kernel::{
    PreparedResidentRegion, PreparedRestoredConstructedCreature, PreparedWorldCorpse,
    PreparedWorldRegionItems, PreparedWorldRegionRoot, ResidentRegionRefresh,
};

mod region_admission;
pub use region_admission::{RegionAdmissionOutcome, RegionAdmissionRequest};

mod vendor_commands;
mod vendor_trees;
pub use vendor_commands::{
    VendorAction, VendorCommand, VendorCommandError, VendorDecision, VendorOutcome,
};
pub use vendor_trees::{
    PreparedVendorLazyItem, PreparedVendorLazyStock, PreparedVendorTree, VendorContents,
    VendorLazyStockReceipt, VendorLazyStockTicket,
};

mod constructed_creatures;
mod generator_mixed;
pub use constructed_creatures::{
    PreparedConstructedCreature, PreparedContainedCreature, PreparedContainedForest,
};
pub use generator_mixed::PreparedMixedGeneratorRoot;

pub mod player_detach;
pub use player_detach::{DetachedPlayer, PlayerDetachOutcome, PlayerDetachRequest};

mod portal_commands;
pub use portal_commands::{PortalResolution, PortalResolutionCommand, PortalResolutionOutcome};

mod equipment_effects;
pub use equipment_effects::{
    EquipmentActivation, EquipmentEffectsPatch, EquipmentItemExperienceChange,
    EquipmentItemPropertyChange, PreparedEquipmentEffectInputs, PreparedEquipmentItemEffects,
};

mod physical_resources;
pub use physical_resources::{
    PhysicalResourceAction, PhysicalResourceCommand, PhysicalResourceDecision,
    PhysicalResourceOutcome, PhysicalResourceTicket,
};

mod magic_resource_commands;
pub use magic_resource_commands::{
    MagicResourceAction, MagicResourceCommand, MagicResourceOutcome, MagicResourceResult,
    PreparedMagicResources,
};

mod equipment_mana;
pub use equipment_mana::{EquipmentManaItem, EquipmentManaRecovery};

mod pk_activity;

mod recall_destinations;
pub use recall_destinations::RecallDestinationSnapshot;

mod player_death_commands;
pub use player_death_commands::{PlayerDeathServiceCommand, PlayerDeathServiceOutcome};

mod social_gags;
pub use social_gags::GagRecovery;

mod npc_combat_assets;
pub use npc_combat_assets::{
    PreparedNpcCombatAssets, PreparedNpcQuality, PreparedNpcRegistryRestore, PreparedNpcSkill,
};
