//! Frozen native NPC operation vocabulary V1. Field/variant order is immutable.
//! Runtime converts explicitly; evolving gameplay structs are never persisted.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcSubjectV1 {
    Source,
    Target,
    Fellowship,
    PetOwner,
    ActivationTarget,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcContextV1 {
    pub source: u32,
    pub target: Option<u32>,
    pub operation: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcPropertyFamilyV1 {
    Bool,
    Int,
    Int64,
    Float,
    String,
    Attribute,
    RawAttribute,
    Vital,
    RawVital,
    Skill,
    RawSkill,
    SkillAdvancement,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcValueV1 {
    Bool(bool),
    Int(i32),
    Int64(i64),
    Float(f64),
    String(String),
    Unsigned(u32),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcQuestMutationV1 {
    Update,
    Stamp,
    Erase,
    Increment(i32),
    Decrement(i32),
    Completions(i32),
    Bits { mask: i32, on: bool },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcPropertyMutationV1 {
    Set {
        family: NpcPropertyFamilyV1,
        value: Option<NpcValueV1>,
    },
    AddInt(i32),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcRewardKindV1 {
    Experience,
    NoShareExperience,
    LevelExperience,
    SkillExperience,
    SkillPoints,
    LevelSkillExperience,
    TrainingCredits,
    Luminance,
    SpendLuminance,
    Vitae,
    RemoveVitae,
    Title,
    TeachSpell,
    UntrainSkill,
    Enlightenment,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcTextKindV1 {
    Act,
    Say,
    Tell,
    Direct,
    Local,
    World,
    FellowBroadcast,
    FellowTell,
    Admin,
    Log,
    Popup,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcDestinationV1 {
    pub cell: Option<u32>,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub relative: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcOperationV1 {
    Text {
        kind: NpcTextKindV1,
        text: String,
        extent: f32,
    },
    Property {
        subject: NpcSubjectV1,
        stat: u32,
        mutation: NpcPropertyMutationV1,
    },
    Quest {
        subject: NpcSubjectV1,
        name: String,
        mutation: NpcQuestMutationV1,
    },
    Reward {
        kind: NpcRewardKindV1,
        amount: i64,
        stat: Option<u32>,
        percent: f64,
        minimum: i64,
        maximum: i64,
    },
    Give {
        template: u32,
        count: u32,
        palette: i32,
        shade: f32,
    },
    Take {
        template: u32,
        count: Option<u32>,
    },
    Cast {
        spell: u32,
        instant: bool,
        pet_owner: bool,
    },
    Motion {
        target: bool,
        motion: u32,
        extent: f32,
        style: Option<u32>,
        substyle: Option<u32>,
    },
    Move {
        home: bool,
        absolute: bool,
        destination: NpcDestinationV1,
        extent: f32,
    },
    Turn {
        target: bool,
        rotation: [f32; 4],
    },
    TeleportTarget(NpcDestinationV1),
    Sanctuary(NpcDestinationV1),
    ResetHome,
    Particle {
        script: u32,
        extent: f32,
    },
    Sound(u32),
    Event {
        name: String,
        start: bool,
    },
    Signal(String),
    Activate,
    Generate,
    DeleteSelf,
    KillSelf,
    OpenSelf(bool),
    LockFellow {
        quest: Option<String>,
    },
    Contract {
        id: u32,
        add: bool,
    },
    Barber,
    Treasure {
        tier: i32,
        category: i32,
        class: i32,
    },
    Confirm {
        key: String,
        text: String,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcCompletionV1 {
    Applied {
        post_delay: f64,
    },
    /// The owning service must return this exact ticket only after actual effect
    /// completion (and confirmed durability for valuable changes).
    Pending {
        ticket: u64,
    },
    /// Independently admitted source chain. Outer script continues after its
    /// actual source post-delay; the owner retains this ticket until completed.
    Detached {
        ticket: u64,
        post_delay: f64,
    },
    /// Quest Update chooses its success/failure branch after mutation completes.
    Branch {
        category: u32,
    },
}
