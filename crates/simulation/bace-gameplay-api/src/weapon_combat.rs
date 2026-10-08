//! Immutable physical-combat preparation and owner-to-owner impact contracts.
//! Runtime adapters prepare these from accepted equipment/content, never packets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PhysicalKind {
    Melee,
    Missile,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalHand {
    Main,
    Offhand,
    Unarmed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PkStatus {
    Protected,
    Npk,
    Pk,
    PkLite,
    Free,
    Baelzharon,
    RubberGlue,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalSkill {
    pub advancement: u32,
    pub current: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalAttackHook {
    pub seconds: f64,
    pub part: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalManeuver {
    pub style: u32,
    pub attack_type: u32,
    pub height: u32,
    pub minimum_skill: u32,
    pub motion: u32,
    pub duration: f64,
    pub hooks: Vec<PhysicalAttackHook>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalWeapon {
    pub entity: u32,
    pub revision: u64,
    pub style: u32,
    pub attack_type: u32,
    pub damage_type: u32,
    pub damage: f64,
    pub variance: f32,
    pub skill: u32,
    pub attack_time: i32,
    pub encumbrance: u32,
    pub offense: f64,
    pub imbues: u32,
    pub biting: f64,
    pub crushing: f64,
    pub slayer_type: u32,
    pub slayer_bonus: f64,
    pub cleave_targets: u32,
    pub ignore_magic_armor: bool,
    pub ignore_magic_resistance: bool,
    pub armor_cleaving: bool,
    pub resistance_cleaving: Option<u32>,
    pub proc_spell: Option<u32>,
    pub proc_chance: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalBodyAttack {
    pub part: u32,
    pub damage: f64,
    pub variance: f32,
    pub damage_type: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalQuality {
    pub raw: f64,
    pub increasing: f64,
    pub decreasing: f64,
    pub additive_increasing: f64,
    pub additive_decreasing: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalResistance {
    pub quality: PhysicalQuality,
    pub augmentation: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalArmorLayer {
    pub armor: PhysicalQuality,
    pub modifiers: [PhysicalQuality; 8],
    pub clothing: bool,
    pub coverage: u32,
    pub shield: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalBodyDefense {
    pub part: u32,
    pub hit_weights: [f32; 12],
    pub armor: [PhysicalQuality; 8],
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhysicalRatings {
    pub damage: i32,
    pub weakness: i32,
    pub critical_damage: i32,
    pub pk_damage: i32,
    pub resistance: i32,
    pub critical_resistance: i32,
    pub pk_resistance: i32,
    pub reckless: i32,
    pub sneak: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalCombatProfile {
    pub equipment: Vec<PhysicalEquipmentStamp>,
    pub revision: u64,
    pub content_hash: [u8; 32],
    pub player: bool,
    pub creature_type: u32,
    pub pk: PkStatus,
    pub attackable: bool,
    pub immune: bool,
    pub lifestone_protected: bool,
    pub style: u32,
    pub main: Option<PhysicalWeapon>,
    pub offhand: Option<PhysicalWeapon>,
    pub launcher: Option<PhysicalWeapon>,
    pub ammunition: Option<PhysicalWeapon>,
    pub gloves: Option<PhysicalWeapon>,
    pub boots: Option<PhysicalWeapon>,
    pub body_attacks: Vec<PhysicalBodyAttack>,
    pub maneuvers: Vec<PhysicalManeuver>,
    pub skills: Vec<(u32, PhysicalSkill)>,
    pub strength: u32,
    pub coordination: u32,
    pub quickness: u32,
    pub base_strength: u32,
    pub base_endurance: u32,
    /// Authoritative motion, equipment, current-target and burden modifiers.
    pub melee_defense_modifier: f64,
    pub missile_defense_modifier: f64,
    pub armor: Vec<PhysicalBodyDefense>,
    pub resistances: [PhysicalResistance; 8],
    pub shield_encumbrance: u32,
    pub shield_placement: bool,
    pub armor_layers: Vec<PhysicalArmorLayer>,
    pub ignore_shield: f64,
    pub shield: Option<PhysicalArmorLayer>,
    pub shield_skill: PhysicalSkill,
    pub ratings: PhysicalRatings,
    pub critical_defense: bool,
    pub range: f32,
    pub height: f32,
    pub missile: Option<PhysicalMissileSpec>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalRolls {
    pub evade: f64,
    pub critical: f64,
    pub variance: f32,
    pub body_part: f32,
    pub attacker_stamina: f64,
    pub defender_stamina: f64,
    pub critical_defense: f32,
    pub sneak: f32,
    pub dirty: f64,
    pub weapon_proc: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalImpact {
    pub damage: u32,
    pub damage_type: u32,
    pub body_part: u32,
    pub critical: bool,
    pub critical_defended: bool,
    pub evaded: bool,
    pub attack_conditions: u32,
    pub attacker_reckless_rating: i32,
    pub attacker_sneak_rating: i32,
    pub dirty_spells: [u32; 2],
    pub dirty_count: usize,
    pub weapon_proc: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalMissileSpec {
    pub ammunition_count: u32,
    pub speed: f32,
    pub radius: f32,
    pub gravity: bool,
    pub tracking: bool,
    pub attack_motion: u32,
    pub launch_seconds: f64,
    pub duration_seconds: f64,
    pub damage_modifier: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalFlight {
    pub cell: u32,
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub radius: f32,
    pub gravity: f32,
    pub lifetime: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalLaunchProposal {
    pub operation: u64,
    pub flight: Option<PhysicalFlight>,
    pub event_id: [u8; 16],
    pub actor: u32,
    pub credited_owner: u32,
    pub target: u32,
    pub projectile: u32,
    pub ammunition: u32,
    pub ammunition_revision: u64,
    pub expected_count: u32,
    pub consumed: u32,
    pub profile_revision: u64,
    pub content_hash: [u8; 32],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalLaunchReceipt {
    pub operation: u64,
    pub ammunition: u32,
    pub before_revision: u64,
    pub after_revision: u64,
    pub remaining: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalEquipmentStamp {
    pub entity: u32,
    pub revision: u64,
    pub location: u32,
}
