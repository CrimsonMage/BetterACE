//! Typed native NPC script ports. Owners must apply/commit before completion.
use bace_geometry::Vec3;
use bace_types::{CellId, EntityId};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcSubject {
    Source,
    Target,
    Fellowship,
    PetOwner,
    ActivationTarget,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NpcContext {
    pub source: EntityId,
    pub target: Option<EntityId>,
    pub operation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcPropertyFamily {
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
#[derive(Clone, Debug, PartialEq)]
pub enum NpcValue {
    Bool(bool),
    Int(i32),
    Int64(i64),
    Float(f64),
    String(String),
    Unsigned(u32),
}
#[derive(Clone, Debug, PartialEq)]
pub enum NpcQuery {
    Property {
        subject: NpcSubject,
        family: NpcPropertyFamily,
        stat: u32,
    },
    Quest {
        subject: NpcSubject,
        name: String,
        check: NpcQuestCheck,
    },
    Event(String),
    FellowCount,
    TitleCount,
    ContractsFull,
    ItemCount {
        template: u32,
    },
    PackSpace {
        containers: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcQuestCheck {
    HasAndCannotSolve,
    Solves {
        minimum: Option<i32>,
        maximum: Option<i32>,
    },
    Bits {
        mask: i32,
        on: bool,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum NpcQueryValue {
    Value(NpcValue),
    Absent,
    NoFellow,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcQuestMutation {
    Update,
    Stamp,
    Erase,
    Increment(i32),
    Decrement(i32),
    Completions(i32),
    Bits { mask: i32, on: bool },
}
#[derive(Clone, Debug, PartialEq)]
pub enum NpcPropertyMutation {
    Set {
        family: NpcPropertyFamily,
        value: Option<NpcValue>,
    },
    AddInt(i32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcRewardKind {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcTextKind {
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcDestination {
    pub cell: Option<CellId>,
    pub position: Vec3,
    pub rotation: [f32; 4],
    pub relative: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum NpcOperation {
    Text {
        kind: NpcTextKind,
        text: String,
        extent: f32,
    },
    Property {
        subject: NpcSubject,
        stat: u32,
        mutation: NpcPropertyMutation,
    },
    Quest {
        subject: NpcSubject,
        name: String,
        mutation: NpcQuestMutation,
    },
    Reward {
        kind: NpcRewardKind,
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
        destination: NpcDestination,
        extent: f32,
    },
    Turn {
        target: bool,
        rotation: [f32; 4],
    },
    TeleportTarget(NpcDestination),
    Sanctuary(NpcDestination),
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NpcCompletion {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcFailure {
    MissingActor,
    MissingContent,
    InvalidInput,
    Capacity,
    DurabilityPending,
    Conflict,
    Unsupported,
    Cancelled,
}
