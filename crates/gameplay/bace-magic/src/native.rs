//! Native spell policy from pinned GDLE extended spell records; ACE supplies the
//! verified DAT header layout and the reviewed Strike classification gap-fill.
use crate::*;
use bace_content::SpellRowV1;
use bace_geometry::Vec3;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeSpellHeader {
    pub id: u32,
    pub school: u32,
    pub category: u32,
    pub flags: u32,
    pub base_mana: u32,
    pub power: u32,
    pub range_constant: f32,
    pub range_modifier: f32,
    pub meta_type: u32,
    pub enchantment: Option<(f64, f32, f32)>,
    pub portal_lifetime: Option<f64>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeProjectileMotion {
    pub template: u32,
    pub radius: f32,
    pub speed: f32,
    pub gravity: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NativeSpellDefinition {
    pub spell: PreparedSpell,
    pub metadata: Option<EnchantmentMetadata>,
    pub fast_resistable_pk_spell: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeSpellError {
    Missing(&'static str),
    Invalid(&'static str),
    Unsupported(&'static str),
}
type E = NativeSpellError;
fn school(value: u32) -> Result<MagicSchool, E> {
    MagicSchool::from_native_id(value).ok_or(E::Invalid("school"))
}

fn vital(value: i32) -> Result<Vital, E> {
    match value {
        2 | 128 => Ok(Vital::Health),
        4 | 256 => Ok(Vital::Stamina),
        6 | 512 => Ok(Vital::Mana),
        _ => Err(E::Invalid("vital")),
    }
}
fn required<T>(value: Option<T>, field: &'static str) -> Result<T, E> {
    value.ok_or(E::Missing(field))
}
fn unsigned(value: Option<i32>, field: &'static str) -> Result<u32, E> {
    u32::try_from(required(value, field)?).map_err(|_| E::Invalid(field))
}
fn enchantment(
    header: NativeSpellHeader,
    row: &SpellRowV1,
) -> Result<(EnchantmentSpec, EnchantmentMetadata), E> {
    let (duration, degrade_modifier, degrade_limit) = if header.meta_type == 15 {
        (required(row.dot_duration, "dot_duration")? + 0.25, 0., 0.)
    } else {
        required(header.enchantment, "enchantment header")?
    };
    let value = required(row.stat_mod_val, "stat_mod_val")?;
    if !duration.is_finite()
        || duration < 0.0
        || !value.is_finite()
        || !degrade_modifier.is_finite()
        || !degrade_limit.is_finite()
    {
        return Err(E::Invalid("enchantment"));
    }
    let beneficial = header.flags & 4 != 0;
    Ok((
        EnchantmentSpec {
            category: u16::try_from(header.category).map_err(|_| E::Invalid("category"))?,
            power: header.power,
            duration,
            layer: 0,
            stat_type: required(row.stat_mod_type, "stat_mod_type")?
                | if beneficial { 0x02000000 } else { 0 },
            stat_key: required(row.stat_mod_key, "stat_mod_key")?,
            value,
            beneficial,
            set_id: None,
        },
        EnchantmentMetadata {
            enchantment_category: header.meta_type,
            has_spell_set_id: false,
            degrade_modifier,
            degrade_limit,
            last_time_degraded: -1.,
            spell_set_id: 0,
        },
    ))
}
fn projectile(
    header: NativeSpellHeader,
    row: &SpellRowV1,
    motion: NativeProjectileMotion,
) -> Result<ProjectileSpec, E> {
    let template = required(row.wcid, "projectile wcid")?;
    if template == 0
        || template != motion.template
        || !motion.radius.is_finite()
        || motion.radius <= 0.0
        || !motion.speed.is_finite()
        || !(0.0..=1000.0).contains(&motion.speed)
        || motion.speed == 0.0
        || !matches!(motion.gravity, 0.0 | 9.8)
    {
        return Err(E::Invalid("projectile assets"));
    }
    let declared = unsigned(row.num_projectiles, "num_projectiles")?;
    let dims =
        [row.dims_origin_x, row.dims_origin_y, row.dims_origin_z].map(|d| -> Result<u16, E> {
            let d = required(d, "projectile dimensions")?;
            if !d.is_finite() || !(0.5..=1024.0).contains(&d) {
                return Err(E::Invalid("projectile dimensions"));
            }
            Ok((d + 0.5) as u16)
        });
    let dimensions = [dims[0]?, dims[1]?, dims[2]?];
    let count = u32::from(dimensions[0]) * u32::from(dimensions[1]) * u32::from(dimensions[2]);
    if count == 0 || count > 1024 || count != declared {
        return Err(E::Invalid("inconsistent projectile count"));
    }
    let spread = row.spread_angle.unwrap_or(0.);
    if !spread.is_finite() || !(0.0..=360.0).contains(&spread) {
        return Err(E::Invalid("spread angle"));
    }
    let c = header.category;
    let shape = if count == 1 {
        if (243..=249).contains(&c) || matches!(c, 639 | 409) {
            ProjectileShape::Streak
        } else if motion.gravity != 0. {
            ProjectileShape::Arc
        } else {
            ProjectileShape::Bolt
        }
    } else if (222..=228).contains(&c) || spread == 360. {
        ProjectileShape::Ring
    } else if (131..=137).contains(&c) || c == 638 {
        ProjectileShape::Blast
    } else if (207..=213).contains(&c) || row.name.contains("Volley") {
        ProjectileShape::Volley
    } else if (229..=235).contains(&c) {
        ProjectileShape::Wall
    } else if (236..=242).contains(&c) {
        ProjectileShape::Strike
    } else {
        ProjectileShape::Group
    };
    let minimum = unsigned(row.base_intensity, "base_intensity")?;
    let variance = unsigned(row.variance, "variance")?;
    let maximum = minimum
        .checked_add(variance)
        .filter(|v| *v <= i32::MAX as u32)
        .ok_or(E::Invalid("projectile damage"))?;
    let offset = Vec3::new(
        row.create_offset_origin_x.unwrap_or(0.),
        row.create_offset_origin_y.unwrap_or(0.),
        row.create_offset_origin_z.unwrap_or(0.),
    );
    let padding = Vec3::new(
        row.padding_origin_x.unwrap_or(0.),
        row.padding_origin_y.unwrap_or(0.),
        row.padding_origin_z.unwrap_or(0.),
    );
    let perturbation = Vec3::new(
        row.peturbation_origin_x.unwrap_or(0.),
        row.peturbation_origin_y.unwrap_or(0.),
        row.peturbation_origin_z.unwrap_or(0.),
    );
    if !offset.is_finite() || !padding.is_finite() || !perturbation.is_finite() {
        return Err(E::Invalid("projectile layout"));
    }
    Ok(ProjectileSpec {
        template,
        shape,
        count: count as u16,
        radius: motion.radius,
        speed: motion.speed,
        gravity: motion.gravity,
        tracking: !row.non_tracking.unwrap_or(false),
        perturbation,
        lifetime: 30.,
        spread_degrees: spread,
        padding,
        offset,
        dimensions,
        minimum_damage: minimum,
        maximum_damage: maximum,
        damage_type: required(row.e_type, "e_type")?,
        enchantment: None,
    })
}
fn sending(row: &SpellRowV1) -> Result<PortalEffect, E> {
    let cell = required(row.position_obj_cell_id, "sending cell")?;
    let position = Vec3::new(
        required(row.position_origin_x, "sending x")?,
        required(row.position_origin_y, "sending y")?,
        required(row.position_origin_z, "sending z")?,
    );
    let [w, x, y, z] = [
        row.position_angles_w,
        row.position_angles_x,
        row.position_angles_y,
        row.position_angles_z,
    ]
    .map(|v| v.unwrap_or(0.));
    if cell == 0
        || !position.is_finite()
        || [w, x, y, z].iter().any(|v| !v.is_finite())
        || (w * w + x * x + y * y + z * z - 1.).abs() > 0.001
    {
        return Err(E::Invalid("sending frame"));
    }
    if x != 0. || y != 0. {
        return Err(E::Unsupported("non-yaw sending frame"));
    }
    Ok(PortalEffect::Sending {
        cell,
        position,
        heading: 2. * z.atan2(w),
    })
}
/// All required server fields are checked before any definition is published.
/// Fields unused by the selected source remain preserved in the native row DTO.
pub fn prepare_native_spell(
    header: NativeSpellHeader,
    row: &SpellRowV1,
    motion: Option<NativeProjectileMotion>,
) -> Result<NativeSpellDefinition, E> {
    if header.id == 0
        || header.id != row.id
        || header.id > u32::from(u16::MAX)
        || header.power > i32::MAX as u32
        || !header.range_constant.is_finite()
        || header.range_constant < 0.
        || !header.range_modifier.is_finite()
        || header.range_modifier < 0.
    {
        return Err(E::Invalid("spell header"));
    }
    let school = school(header.school)?;
    let mut metadata = None;
    let effect = match header.meta_type {
        1 | 12 => {
            let (spec, meta) = enchantment(header, row)?;
            metadata = Some(meta);
            if header.meta_type == 1 {
                SpellEffect::Enchantment(spec)
            } else {
                SpellEffect::FellowshipEnchantment(spec)
            }
        }
        2 | 10 | 15 => {
            let mut spec = projectile(header, row, required(motion, "projectile assets")?)?;
            if header.meta_type == 15 {
                let (enchantment, meta) = enchantment(header, row)?;
                spec.enchantment = Some(enchantment);
                metadata = Some(meta);
            }
            if header.meta_type == 10 {
                let proportion = required(row.drain_percentage, "drain_percentage")?;
                let damage_ratio = required(row.damage_ratio, "damage_ratio")?;
                if !proportion.is_finite()
                    || !(0.0..=1.0).contains(&proportion)
                    || !damage_ratio.is_finite()
                    || !(0.0..=1000.0).contains(&damage_ratio)
                {
                    return Err(E::Invalid("life projectile"));
                }
                SpellEffect::LifeProjectile {
                    source: vital(spec.damage_type as i32)?,
                    projectile: spec,
                    proportion,
                    damage_ratio,
                }
            } else {
                SpellEffect::Projectile(spec)
            }
        }
        3 | 11 => {
            let minimum = required(row.boost, "boost")?;
            let maximum = minimum
                .checked_add(required(row.boost_variance, "boost_variance")?)
                .ok_or(E::Invalid("boost overflow"))?;
            let vital = vital(required(row.damage_type, "damage_type")?)?;
            if header.meta_type == 11 && (minimum < 0 || maximum < 0) {
                return Err(E::Unsupported("harmful fellowship boost"));
            }
            if header.meta_type == 3 {
                SpellEffect::Boost {
                    vital,
                    minimum,
                    maximum,
                }
            } else {
                SpellEffect::FellowshipBoost {
                    vital,
                    minimum,
                    maximum,
                }
            }
        }
        4 => {
            let flags = required(row.transfer_bitfield, "transfer_bitfield")?;
            let source_is_caster = match flags & 3 {
                1 => true,
                2 => false,
                _ => return Err(E::Invalid("transfer source")),
            };
            let destination_is_caster = match flags & 12 {
                4 => true,
                8 => false,
                _ => return Err(E::Invalid("transfer destination")),
            };
            let proportion = required(row.proportion, "proportion")?;
            let loss = required(row.loss_percent, "loss_percent")?;
            if !proportion.is_finite()
                || !(0.0..=1.0).contains(&proportion)
                || !loss.is_finite()
                || !(0.0..=1.0).contains(&loss)
            {
                return Err(E::Invalid("transfer proportions"));
            }
            SpellEffect::Transfer {
                source: vital(required(row.source, "source")?)?,
                destination: vital(required(row.destination, "destination")?)?,
                source_is_caster,
                destination_is_caster,
                proportion,
                loss,
                cap: unsigned(row.transfer_cap, "transfer_cap")?,
            }
        }
        5 => SpellEffect::Portal(PortalEffect::Link {
            slot: unsigned(row.index, "link index")?,
        }),
        6 => SpellEffect::Portal(PortalEffect::Recall {
            slot: unsigned(row.index, "recall index")?,
        }),
        7 => {
            let lifetime = required(header.portal_lifetime, "portal lifetime")?;
            if !lifetime.is_finite() || lifetime <= 0. {
                return Err(E::Invalid("portal lifetime"));
            }
            SpellEffect::Portal(PortalEffect::Summon {
                slot: unsigned(row.link, "portal link")?,
                template: 0x7a3,
                lifetime,
            })
        }
        8 => SpellEffect::Portal(sending(row)?),
        13 => SpellEffect::FellowshipPortal(sending(row)?),
        9 | 14 => {
            if row.number_variance.unwrap_or(0.) != 0. || row.power_variance.unwrap_or(0.) != 0. {
                return Err(E::Unsupported("variable dispel selection"));
            }
            let min = unsigned(row.min_power, "min_power")?;
            let max = unsigned(row.max_power, "max_power")?;
            if min > max {
                return Err(E::Invalid("dispel power"));
            }
            let count = match required(row.number, "dispel number")? {
                -1 => u32::MAX,
                v => u32::try_from(v).map_err(|_| E::Invalid("dispel number"))?,
            };
            let align = required(row.align, "dispel alignment")?;
            if !(0..=2).contains(&align) {
                return Err(E::Invalid("dispel alignment"));
            }
            let school = match row.dispel_school.unwrap_or(0) {
                0 => None,
                s => Some(crate::native::school(s as u32)?),
            };
            let spec = DispelSpec {
                minimum_power: min,
                maximum_power: max,
                count,
                harmful: align != 1,
                beneficial: align != 2,
                school,
            };
            if header.meta_type == 9 {
                SpellEffect::Dispel(spec)
            } else {
                SpellEffect::FellowshipDispel(spec)
            }
        }
        _ => return Err(E::Unsupported("spell family")),
    };
    Ok(NativeSpellDefinition {
        spell: PreparedSpell {
            id: header.id,
            school,
            power: header.power,
            base_mana: header.base_mana,
            range_constant: header.range_constant,
            range_per_skill: header.range_modifier,
            harmful: header.flags & 4 == 0,
            resistable: header.flags & 1 != 0,
            effect,
        },
        metadata,
        fast_resistable_pk_spell: header.flags & (1 | 2 | 0x4000) == 1 | 2 | 0x4000,
    })
}
