//! Pure accepted-content preparation. DAT decoding and mutable item ownership are
//! supplied by adapters; GDLE property defaults/equipment semantics remain here.
use crate::physical::{PhysicalError, validate_physical_profile};
use bace_content::WeenieV1;
use bace_gameplay_api::weapon_combat::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalQualityFamily {
    Int,
    Float,
    BodyArmor,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalQualityProjection {
    pub entity: u32,
    pub family: PhysicalQualityFamily,
    pub stat: u32,
    pub part: Option<u32>,
    pub details: PhysicalQuality,
    /// Already source-ordered int/float enchantment result. Armor/resistance use
    /// details instead, preserving the separate GDLE EnchantedQualityDetails path.
    pub value: f64,
}
pub struct PhysicalEquipment<'a> {
    pub entity: u32,
    pub revision: u64,
    pub location: u32,
    pub weenie: &'a WeenieV1,
}
/// Accepted immutable source closure retained for registry-derived refreshes.
/// Derived profiles cannot reconstruct raw weapon/rating properties losslessly.
#[derive(Clone, Debug)]
pub struct PreparedPhysicalRefreshSource {
    pub actor: u32,
    pub weenie: std::sync::Arc<WeenieV1>,
    pub equipment: Vec<PhysicalEquipmentSource>,
    pub qualities: Vec<PhysicalQualityProjection>,
    pub profile: std::sync::Arc<PhysicalCombatProfile>,
}
#[derive(Clone, Debug)]
pub struct PhysicalEquipmentSource {
    pub entity: u32,
    pub revision: u64,
    pub location: u32,
    pub weenie: std::sync::Arc<WeenieV1>,
}
pub fn refresh_physical_from_source(
    source: &PreparedPhysicalRefreshSource,
    current: &PhysicalCombatProfile,
    qualities: &[PhysicalQualityProjection],
) -> Result<PhysicalCombatProfile, PhysicalError> {
    if source.actor == 0
        || source.equipment.len() > 128
        || qualities.len() > 32768
        || source.profile.equipment != current.equipment
    {
        return Err(PhysicalError::InvalidInput);
    }
    let equipment: Vec<_> = source
        .equipment
        .iter()
        .map(|e| PhysicalEquipment {
            entity: e.entity,
            revision: e.revision,
            location: e.location,
            weenie: &e.weenie,
        })
        .collect();
    let mut profile = prepare_physical(PhysicalPreparation {
        actor: source.actor,
        revision: current.revision,
        content_hash: current.content_hash,
        player: current.player,
        weenie: &source.weenie,
        equipment: &equipment,
        skills: &current.skills,
        attributes: [
            current.strength,
            0,
            current.quickness,
            current.coordination,
            0,
            0,
        ],
        base_attributes: [current.base_strength, current.base_endurance, 0, 0, 0, 0],
        qualities,
        enchantments_complete: true,
        maneuvers: source.profile.maneuvers.clone(),
        height: source.profile.height,
        missile: current.missile,
        melee_defense_modifier: current.melee_defense_modifier,
        missile_defense_modifier: current.missile_defense_modifier,
    })?;
    if profile.equipment != current.equipment {
        return Err(PhysicalError::InvalidInput);
    }
    profile.range = source.profile.range;
    Ok(profile)
}
pub struct PhysicalPreparation<'a> {
    pub actor: u32,
    pub revision: u64,
    pub content_hash: [u8; 32],
    pub player: bool,
    pub weenie: &'a WeenieV1,
    pub equipment: &'a [PhysicalEquipment<'a>],
    pub skills: &'a [(u32, PhysicalSkill)],
    pub attributes: [u32; 6],
    pub base_attributes: [u32; 6],
    pub qualities: &'a [PhysicalQualityProjection],
    pub enchantments_complete: bool,
    pub maneuvers: Vec<PhysicalManeuver>,
    pub height: f32,
    pub missile: Option<PhysicalMissileSpec>,
    pub melee_defense_modifier: f64,
    pub missile_defense_modifier: f64,
}
fn int(w: &WeenieV1, id: u32, default: i32) -> i32 {
    w.properties
        .ints
        .iter()
        .find(|p| p.id == id)
        .map_or(default, |p| p.value)
}
fn float(w: &WeenieV1, id: u32, default: f64) -> f64 {
    w.properties
        .floats
        .iter()
        .find(|p| p.id == id)
        .map_or(default, |p| p.value)
}
fn boolean(w: &WeenieV1, id: u32, default: bool) -> bool {
    w.properties
        .bools
        .iter()
        .find(|p| p.id == id)
        .map_or(default, |p| p.value)
}
fn did(w: &WeenieV1, id: u32) -> Option<u32> {
    w.properties
        .data_ids
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.value)
}
fn quality(raw: f64) -> PhysicalQuality {
    PhysicalQuality {
        raw,
        increasing: 1.0,
        decreasing: 1.0,
        additive_increasing: 0.0,
        additive_decreasing: 0.0,
    }
}
struct View<'a> {
    projections: &'a [PhysicalQualityProjection],
}
impl View<'_> {
    fn q(
        &self,
        entity: u32,
        family: PhysicalQualityFamily,
        stat: u32,
        part: Option<u32>,
        raw: f64,
    ) -> PhysicalQuality {
        self.projections
            .iter()
            .find(|p| p.entity == entity && p.family == family && p.stat == stat && p.part == part)
            .map_or_else(|| quality(raw), |p| p.details)
    }
    fn v(&self, entity: u32, family: PhysicalQualityFamily, stat: u32, raw: f64) -> f64 {
        self.projections
            .iter()
            .find(|p| {
                p.entity == entity && p.family == family && p.stat == stat && p.part.is_none()
            })
            .map_or(raw, |p| p.value)
    }
}
fn checked_u32(value: f64) -> Result<u32, PhysicalError> {
    if !value.is_finite() || value < 0.0 || value > f64::from(u32::MAX) {
        Err(PhysicalError::InvalidInput)
    } else {
        Ok(value as u32)
    }
}
fn checked_i32(value: f64) -> Result<i32, PhysicalError> {
    if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        Err(PhysicalError::InvalidInput)
    } else {
        Ok(value as i32)
    }
}
fn weapon(
    e: &PhysicalEquipment<'_>,
    actor: u32,
    owner: &WeenieV1,
    player: bool,
    v: &View<'_>,
) -> Result<PhysicalWeapon, PhysicalError> {
    use PhysicalQualityFamily::{Float, Int};
    let w = e.weenie;
    let inherit = int(w, 36, 0) < 9999;
    let owner_damage = v.v(actor, Int, 44, f64::from(int(owner, 44, 0)))
        + if player {
            v.v(actor, Int, 360, 0.0)
        } else {
            0.0
        };
    let own_damage = v.v(e.entity, Int, 44, f64::from(int(w, 44, 0)));
    let owner_speed = if player {
        v.v(actor, Int, 361, 0.0)
    } else {
        v.v(actor, Int, 49, f64::from(int(owner, 49, 0)))
    };
    let owner_offense = if player {
        v.v(actor, Float, 168, 0.0)
    } else {
        v.v(actor, Float, 62, float(owner, 62, 0.0))
    };
    let damage = own_damage + if inherit { owner_damage } else { 0.0 };
    let speed =
        v.v(e.entity, Int, 49, f64::from(int(w, 49, 0))) + if inherit { owner_speed } else { 0.0 };
    let imbues = [179, 303, 304, 305, 306]
        .into_iter()
        .fold(0, |bits, id| bits | int(w, id, 0) as u32);
    Ok(PhysicalWeapon {
        entity: e.entity,
        revision: e.revision,
        style: int(w, 46, 0) as u32,
        attack_type: int(w, 47, 0) as u32,
        damage_type: int(w, 45, 0) as u32,
        damage: f64::from(checked_i32(damage)?),
        variance: float(w, 22, 0.0) as f32,
        skill: match int(w, 48, 45) {
            11 => 44,
            4 => 46,
            1 | 5 | 9 | 10 | 13 => 45,
            2 | 3 | 12 => 47,
            id => id as u32,
        },
        attack_time: checked_i32(speed)?,
        encumbrance: checked_u32(f64::from(int(w, 5, 0)))?,
        offense: v.v(e.entity, Float, 62, float(w, 62, 0.0))
            + if inherit { owner_offense } else { 0.0 },
        imbues,
        biting: v.v(e.entity, Float, 147, float(w, 147, 0.0)),
        crushing: v.v(e.entity, Float, 136, float(w, 136, 0.0)),
        slayer_type: int(w, 166, 0) as u32,
        slayer_bonus: v.v(e.entity, Float, 138, float(w, 138, 0.0)),
        cleave_targets: checked_u32(f64::from(int(w, 292, 1)))?.max(1),
        ignore_magic_armor: boolean(w, 66, false) || boolean(owner, 66, false),
        ignore_magic_resistance: boolean(w, 65, false) || boolean(owner, 65, false),
        armor_cleaving: float(w, 155, 0.0) != 0.0,
        resistance_cleaving: w
            .properties
            .ints
            .iter()
            .find(|p| p.id == 263)
            .map(|p| p.value as u32),
        proc_spell: did(w, 55),
        proc_chance: float(w, 156, 0.0),
    })
}
fn armor_layer(entity: u32, w: &WeenieV1, shield: bool, v: &View<'_>) -> PhysicalArmorLayer {
    use PhysicalQualityFamily::{Float, Int};
    let armor_id = if shield { 56 } else { 28 };
    PhysicalArmorLayer {
        armor: v.q(entity, Int, armor_id, None, f64::from(int(w, armor_id, 0))),
        modifiers: [13, 14, 15, 16, 17, 18, 19, 165]
            .map(|id| v.q(entity, Float, id, None, float(w, id, 1.0))),
        clothing: w.weenie_type == 2,
        coverage: int(w, 9, 0) as u32,
        shield,
    }
}
pub fn prepare_physical(
    input: PhysicalPreparation<'_>,
) -> Result<PhysicalCombatProfile, PhysicalError> {
    use PhysicalQualityFamily::{BodyArmor, Float, Int};
    if input.actor == 0
        || !input.enchantments_complete
        || input.equipment.len() > 64
        || input.qualities.len() > 4096
        || input.skills.len() > 256
        || input.qualities.iter().any(|p| {
            !p.value.is_finite()
                || ![
                    p.details.raw,
                    p.details.increasing,
                    p.details.decreasing,
                    p.details.additive_increasing,
                    p.details.additive_decreasing,
                ]
                .into_iter()
                .all(f64::is_finite)
        })
    {
        return Err(PhysicalError::InvalidInput);
    }
    for (index, e) in input.equipment.iter().enumerate() {
        if e.entity == 0
            || e.entity == input.actor
            || e.location == 0
            || input.equipment[..index]
                .iter()
                .any(|old| old.entity == e.entity)
        {
            return Err(PhysicalError::InvalidInput);
        }
    }
    for (index, p) in input.qualities.iter().enumerate() {
        if p.entity != input.actor && !input.equipment.iter().any(|e| e.entity == p.entity)
            || input.qualities[..index].iter().any(|q| {
                q.entity == p.entity && q.family == p.family && q.stat == p.stat && q.part == p.part
            })
        {
            return Err(PhysicalError::InvalidInput);
        }
    }
    let v = View {
        projections: input.qualities,
    };
    let w = input.weenie;
    let slot = |mask: u32| -> Result<Option<&PhysicalEquipment<'_>>, PhysicalError> {
        let mut found = input.equipment.iter().filter(|e| e.location & mask != 0);
        let result = found.next();
        if found.next().is_some() {
            return Err(PhysicalError::InvalidInput);
        }
        Ok(result)
    };
    let main_item = slot(0x100000 | 0x2000000)?;
    let left = slot(0x200000)?;
    let launcher_item = slot(0x400000)?;
    let ammo_item = slot(0x800000)?;
    let mut main = main_item
        .map(|e| weapon(e, input.actor, w, input.player, &v))
        .transpose()?;
    let (shield_item, offhand_item) = match left {
        Some(e) if int(e.weenie, 9, 0) == 0x200000 => (Some(e), None),
        e => (None, e),
    };
    let offhand = offhand_item
        .map(|e| weapon(e, input.actor, w, input.player, &v))
        .transpose()?;
    let launcher = launcher_item
        .map(|e| weapon(e, input.actor, w, input.player, &v))
        .transpose()?;
    let mut ammunition = ammo_item
        .map(|e| weapon(e, input.actor, w, input.player, &v))
        .transpose()?;
    if let (Some(ammo), Some(raw_ammo), Some(launch), Some(raw_launcher)) =
        (&mut ammunition, ammo_item, &launcher, launcher_item)
    {
        // Ammunition is not enchanted: raw damage + enchanted launcher + matching elemental bonus.
        ammo.damage = f64::from(int(raw_ammo.weenie, 44, 0)) + launch.damage;
        if ammo.damage_type == 0 || ammo.damage_type == launch.damage_type {
            ammo.damage += v.v(
                raw_launcher.entity,
                Int,
                204,
                f64::from(int(raw_launcher.weenie, 204, 0)),
            );
        }
    }
    let style = if let Some(weapon) = &launcher {
        match weapon.style {
            0x10 => 0x8000003f,
            0x20 => 0x80000041,
            0x40 => 0x80000043,
            0x80 | 0x800 => 0x80000047,
            0x400 => 0x8000013b,
            _ => return Err(PhysicalError::InvalidInput),
        }
    } else {
        match main.as_ref().map_or(0, |w| w.style) {
            0 | 1 => {
                if offhand.is_some() {
                    0x80000046
                } else {
                    0x8000003c
                }
            }
            2 => {
                if offhand.is_some() {
                    0x80000046
                } else if shield_item.is_some() {
                    0x80000040
                } else {
                    0x8000003e
                }
            }
            4 => 0x80000040,
            8 => 0x80000044,
            0x100 => 0x80000046,
            _ => return Err(PhysicalError::InvalidInput),
        }
    };
    if let Some(main) = &mut main
        && main.skill == 0
    {
        main.skill = 45;
    }
    let mut body_attacks = Vec::with_capacity(w.properties.body_parts.len());
    let mut armor = Vec::with_capacity(w.properties.body_parts.len());
    for property in &w.properties.body_parts {
        let part = u32::try_from(property.id).map_err(|_| PhysicalError::InvalidInput)?;
        let b = &property.value;
        body_attacks.push(PhysicalBodyAttack {
            part,
            damage: f64::from(b.d_val),
            variance: b.d_var,
            damage_type: b.d_type as u32,
        });
        let values = [
            b.armor_vs_slash,
            b.armor_vs_pierce,
            b.armor_vs_bludgeon,
            b.armor_vs_cold,
            b.armor_vs_fire,
            b.armor_vs_acid,
            b.armor_vs_electric,
            b.armor_vs_nether,
        ];
        let types = [1, 2, 4, 8, 16, 32, 64, 1024];
        armor.push(PhysicalBodyDefense {
            part,
            hit_weights: [
                b.llf, b.llb, b.lrf, b.lrb, b.mlf, b.mlb, b.mrf, b.mrb, b.hlf, b.hlb, b.hrf, b.hrb,
            ],
            armor: std::array::from_fn(|i| {
                v.q(
                    input.actor,
                    BodyArmor,
                    types[i],
                    Some(part),
                    f64::from(values[i]),
                )
            }),
        });
    }
    body_attacks.sort_by_key(|p| p.part);
    armor.sort_by_key(|p| p.part);
    let mut armor_layers = vec![armor_layer(input.actor, w, false, &v)];
    for e in input
        .equipment
        .iter()
        .filter(|e| e.location & (0x80001ff | 0x7e00) != 0)
    {
        armor_layers.push(armor_layer(e.entity, e.weenie, false, &v));
    }
    let resist_ids = [64, 65, 66, 68, 67, 69, 70, 166];
    let aug_ids = [240, 241, 242, 245, 244, 243, 246, 327];
    let resistances = std::array::from_fn(|i| PhysicalResistance {
        quality: v.q(
            input.actor,
            Float,
            resist_ids[i],
            None,
            float(w, resist_ids[i], 1.0),
        ),
        augmentation: if int(w, 239, 0) > 0 {
            int(w, aug_ids[i], 0).max(0) as u32
        } else {
            0
        },
    });
    let rating = |ids: &[u32]| -> Result<i32, PhysicalError> {
        let mut sum = 0i64;
        for id in ids {
            sum = sum
                .checked_add(i64::from(checked_i32(v.v(
                    input.actor,
                    Int,
                    *id,
                    f64::from(int(w, *id, 0)),
                ))?))
                .ok_or(PhysicalError::Overflow)?;
        }
        i32::try_from(sum).map_err(|_| PhysicalError::Overflow)
    };
    let ratings = PhysicalRatings {
        damage: rating(&[307, 370, 309, 333])?,
        weakness: rating(&[329])?,
        critical_damage: rating(&[314, 374, 299, 335])?,
        pk_damage: rating(&[381, 383])?,
        resistance: rating(&[308, 371, 310, 334])?,
        critical_resistance: rating(&[316, 375, 336])?,
        pk_resistance: rating(&[382, 384])?,
        reckless: rating(&[357])?,
        sneak: rating(&[356])?,
    };
    let shield_skill = input.skills.iter().find(|s| s.0 == 48).map_or(
        PhysicalSkill {
            advancement: 0,
            current: 0,
        },
        |s| s.1,
    );
    let mut equipment: Vec<_> = input
        .equipment
        .iter()
        .map(|e| PhysicalEquipmentStamp {
            entity: e.entity,
            revision: e.revision,
            location: e.location,
        })
        .collect();
    equipment.sort_by_key(|e| e.entity);
    let mut missile = input.missile;
    if let Some(m) = &mut missile {
        m.gravity = true;
    }
    let profile = PhysicalCombatProfile {
        equipment,
        revision: input.revision,
        content_hash: input.content_hash,
        player: input.player,
        creature_type: int(w, 2, 0) as u32,
        pk: match int(w, 134, 2) {
            4 => PkStatus::Pk,
            64 => PkStatus::PkLite,
            32 => PkStatus::Free,
            1 => PkStatus::Protected,
            16 => PkStatus::RubberGlue,
            _ => PkStatus::Npk,
        },
        attackable: boolean(w, 19, true),
        immune: boolean(w, 98, false),
        lifestone_protected: boolean(w, 30, false),
        style,
        main,
        offhand,
        launcher,
        ammunition,
        gloves: slot(0x20)?
            .map(|e| weapon(e, input.actor, w, input.player, &v))
            .transpose()?,
        boots: slot(0x100)?
            .map(|e| weapon(e, input.actor, w, input.player, &v))
            .transpose()?,
        body_attacks,
        maneuvers: input.maneuvers,
        skills: input.skills.to_vec(),
        strength: input.attributes[0],
        coordination: input.attributes[3],
        quickness: input.attributes[2],
        base_strength: input.base_attributes[0],
        base_endurance: input.base_attributes[1],
        melee_defense_modifier: input.melee_defense_modifier,
        missile_defense_modifier: input.missile_defense_modifier,
        armor,
        resistances,
        shield_encumbrance: shield_item.map_or(0, |e| int(e.weenie, 5, 0).max(0) as u32),
        shield_placement: shield_item.is_some(),
        armor_layers,
        ignore_shield: v.v(input.actor, Float, 151, float(w, 151, 0.0)),
        shield: shield_item.map(|e| armor_layer(e.entity, e.weenie, true, &v)),
        shield_skill,
        ratings,
        critical_defense: int(w, 233, 0) != 0,
        range: 2.0,
        height: input.height,
        missile,
    };
    validate_physical_profile(&profile)?;
    Ok(profile)
}
