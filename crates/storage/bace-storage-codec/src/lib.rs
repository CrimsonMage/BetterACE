//! Bounded, integrity-checked storage envelopes for frozen DTO schemas.
mod bounded_output;
mod envelope;
mod mapping;
mod pack_format;
mod pack_generation;
mod pack_manifest;
mod pack_reader;
mod pack_writer;

pub use envelope::{CodecError, CodecLimits, EnvelopeInfo, decode, encode, inspect};
pub use pack_format::{
    MAX_ACTIVE_PACKS, PackDescriptor, PackError, PackKey, PackLimits, PackRecord,
};
pub use pack_generation::PackGeneration;
pub use pack_manifest::{PackManifest, load_manifest, write_manifest};
pub use pack_reader::{MappedPack, PackLookup, RecordHandle};
pub use pack_writer::compile_pack;

mod gameplay_save;
pub use gameplay_save::{
    CorpseSaveV1, EntitySaveV1, HouseAccessV1, HouseSaveV1, PlayerSaveV1, QuestSaveV1,
    SaveCodecError,
};

pub use gameplay_save::{CharacterMetadataV1, SpellFavoriteV1};
mod item_v2;
pub use item_v2::{ItemPlacementV2, ItemSaveV2};
mod house_v2;
pub use house_v2::{HousePaymentV2, HouseSaveV2};
mod rare_save;
pub use rare_save::RareStateV1;
mod enchantment_save;
pub use enchantment_save::{FrozenEnchantmentV1, validate_enchantments_v1};
mod player_v2;
pub use player_v2::PlayerSaveV2;
mod corpse_v2;
pub use corpse_v2::CorpseSaveV2;

mod ui_save;
pub use ui_save::{CharacterUiV1, ComponentPreferenceV1, ShortcutSaveV1};
mod saves_v3;
pub use saves_v3::{CorpseSaveV3, HouseSaveV3, ItemSaveV3, PlayerSaveV3};

mod combat_recovery_v1;
pub use combat_recovery_v1::FrozenCombatRecoveryV1;

mod player_v4;
pub use player_v4::{CombatRecoverySaveV1, ContractSaveV1, PlayerSaveV4};

mod recovery_transition;
pub use recovery_transition::validate_recovery_transition;

pub mod npc_values_v1;

pub mod npc_workflow_v1;
mod npc_workflow_validation;
pub use npc_workflow_v1::NpcWorkflowSaveV1;
pub mod npc_workflow_v2;
pub use npc_workflow_v2::{NpcExperienceSharingV2, NpcSharingPolicyV2, NpcWorkflowSaveV2};
pub mod npc_workflow_v3;
pub use npc_workflow_v3::{NpcArchivedPropertyV3, NpcSourceArchiveV3, NpcWorkflowSaveV3};

pub mod npc_effects_v1;

pub mod social_v1;
pub use social_v1::{SocialSaveV1, SocialSquelchV1};
pub mod allegiance_v1;
pub use allegiance_v1::{AllegianceMetadataV1, AllegianceNodeV1, AllegianceSanctuaryV1};

mod player_v5;
pub use player_v5::PlayerSaveV5;
mod player_v6;
pub use player_v6::{PhysicalRecoverySaveV1, PlayerSaveV6, validate_physical_recovery_transition};

mod corpse_v4;
pub use corpse_v4::{CorpseSaveV4, validate_corpse_transition};
mod corpse_v5;
pub use corpse_v5::{CorpseAccessSaveV1, CorpseSaveV5, validate_corpse_transition_v5};

mod pve_death_receipt_v1;
pub use pve_death_receipt_v1::{PVE_DEATH_RECEIPT_KIND, PveDeathReceiptV1};

mod item_v4;
pub use item_v4::{
    FrozenConstructedChildV1, FrozenCreatureConstructionV1, FrozenGeneratorConstructionOriginV1,
    ItemSaveV4, validate_item_construction_transition,
};
mod item_v5;
pub use item_v5::{ItemSaveV5, validate_item_source_destination_transition};

mod vendor_stock_v1;
pub use vendor_stock_v1::{
    VENDOR_STOCK_KIND, VendorDefaultStockV1, VendorStockSaveV1, VendorUniqueStockV1,
};
