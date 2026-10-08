//! Pure boundary from verified decoded DAT/content to the character domain.
//! This performs no reads and grants no geometry admission, account privileges or
//! name approval. Supplementary starter rules must come from admitted native data.
use crate::character_assets::PreparedCharacterAssets;
use bace_auth::AccountRecord;
use bace_character::{
    AppearanceSelection, CharacterCreateRequest, CharacterStart, CharacterTemplate,
    ClothingSelection, FaceTextures, PreparedCreationAssets, PreparedEyes, PreparedGender,
    PreparedHair, SkillGear, SkillSpell, StarterItem, VitalFormula,
};
use bace_content::{Position, WeenieV1};
use bace_dat::{
    ClothingTable, CreationGender, DatPaletteSet, ObjectDescription, SkillFormula, VitalTable,
};
use bace_gameplay_api::{CreationAllocation, CreationAttributes, SkillAdvancement};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CreationPreparationError {
    #[error("invalid authenticated character identity")]
    Identity,
    #[error("expected 55 skill entries, got {0}")]
    SkillCount(usize),
    #[error("invalid skill advancement {value} at slot {slot}")]
    Skill { slot: usize, value: u32 },
    #[error("invalid negative template index")]
    TemplateIndex,
    #[error("missing heritage {0}")]
    Heritage(u32),
    #[error("missing required native supplement: {0}")]
    Supplement(&'static str),
    #[error("missing verified palette set {0:#x}")]
    Palette(u32),
    #[error("missing accepted template {0}")]
    Template(u32),
    #[error("missing verified clothing table for template {template}: {table:#x}")]
    Clothing { template: u32, table: u32 },
    #[error("missing character face texture: gender {gender}, family {family}, index {index}")]
    Face {
        gender: u32,
        family: &'static str,
        index: usize,
    },
    #[error("missing admitted starter area {0}")]
    Start(u32),
    #[error("invalid prepared data: {0}")]
    Invalid(&'static str),
    #[error("creation preparation count limit")]
    Capacity,
}
/// Immutable content revision resolved from the accepted mapped generation.
pub struct CreationTemplateInput {
    pub revision: u64,
    pub template: WeenieV1,
}
/// The location uses the first verified DAT start location, as PlayerFactory does.
/// Instantiation comes from admitted spell/start configuration; no fallback guessed.
pub struct AdmittedStart {
    pub area: u32,
    pub instantiation: Position,
}
pub struct CreationSupplementary<'a> {
    pub vitals: &'a VitalTable,
    pub palettes: &'a BTreeMap<u32, DatPaletteSet>,
    pub clothing: &'a BTreeMap<u32, ClothingTable>,
    pub human: &'a CreationTemplateInput,
    pub items: &'a BTreeMap<u32, CreationTemplateInput>,
    /// None is unsupported/unprepared, distinct from an explicitly admitted empty set.
    pub skill_gear: Option<&'a [SkillGear]>,
    pub skill_spells: Option<&'a [SkillSpell]>,
    pub starts: Option<&'a [AdmittedStart]>,
}
/// Call only for the current authenticated generation with a server-allocated ID.
/// Client account text is consistency metadata; requested admin/sentinel flags
/// are deliberately absent from the domain intent and cannot grant privileges.
pub fn creation_request(
    wire: &bace_wire::CharacterCreateRequest,
    account: &AccountRecord,
    entity: u32,
) -> Result<CharacterCreateRequest, CreationPreparationError> {
    use CreationPreparationError as Error;
    if account.disabled
        || account.id.0 == 0
        || !(0x50000001..=0x5fffffff).contains(&entity)
        || bace_auth::AccountName::parse(&wire.account).ok().as_ref() != Some(&account.name)
    {
        return Err(Error::Identity);
    }
    if wire.skill_advancement_classes.len() != 55 {
        return Err(Error::SkillCount(wire.skill_advancement_classes.len()));
    }
    let mut skills = [SkillAdvancement::Inactive; 55];
    for (slot, &value) in wire.skill_advancement_classes.iter().enumerate() {
        skills[slot] =
            SkillAdvancement::try_from(value).map_err(|_| Error::Skill { slot, value })?;
    }
    let a = &wire.abilities;
    let p = &wire.appearance;
    Ok(CharacterCreateRequest {
        account: account.id.0,
        entity,
        name: wire.name.clone(),
        heritage: wire.heritage,
        gender: wire.gender,
        template_index: u32::try_from(wire.template_option).map_err(|_| Error::TemplateIndex)?,
        start_area: wire.start_area,
        allocation: CreationAllocation {
            attributes: CreationAttributes {
                strength: a.strength,
                endurance: a.endurance,
                coordination: a.coordination,
                quickness: a.quickness,
                focus: a.focus,
                self_attribute: a.self_ability,
            },
            skills,
        },
        appearance: AppearanceSelection {
            hair_style: p.hair_style,
            hair_color: p.hair_color,
            hair_hue: p.hair_hue,
            skin_hue: p.skin_hue,
            eyes: p.eyes,
            eye_color: p.eye_color,
            nose: p.nose,
            mouth: p.mouth,
            clothing: [
                ClothingSelection {
                    style: p.headgear_style,
                    color: p.headgear_color,
                    hue: p.headgear_hue,
                },
                ClothingSelection {
                    style: p.shirt_style,
                    color: p.shirt_color,
                    hue: p.shirt_hue,
                },
                ClothingSelection {
                    style: p.pants_style,
                    color: p.pants_color,
                    hue: p.pants_hue,
                },
                ClothingSelection {
                    style: p.footwear_style,
                    color: p.footwear_color,
                    hue: p.footwear_hue,
                },
            ],
        },
    })
}
pub fn prepare_creation_assets(
    base: &PreparedCharacterAssets,
    heritage: u32,
    input: CreationSupplementary<'_>,
) -> Result<PreparedCreationAssets, CreationPreparationError> {
    use CreationPreparationError as Error;
    let group = base
        .char_gen()
        .heritage_groups
        .get(&heritage)
        .ok_or(Error::Heritage(heritage))?;
    let rules = base.creation(heritage).ok_or(Error::Heritage(heritage))?;
    let gear = input.skill_gear.ok_or(Error::Supplement("starter gear"))?;
    let spells = input
        .skill_spells
        .ok_or(Error::Supplement("starter spells"))?;
    let starts = input.starts.ok_or(Error::Supplement("admitted starts"))?;
    if starts.is_empty() {
        return Err(Error::Supplement("admitted starts"));
    }
    if group.genders.len() > 8
        || group.templates.len() > 256
        || starts.len() > 256
        || gear.len() > 4096
        || spells.len() > 4096
    {
        return Err(Error::Capacity);
    }
    if input.human.revision == 0 {
        return Err(Error::Invalid("human template revision"));
    }
    input
        .human
        .template
        .validate(Default::default())
        .map_err(|_| Error::Invalid("human template"))?;
    let mut genders = Vec::with_capacity(group.genders.len());
    let mut needed = BTreeSet::new();
    let mut wardrobe = BTreeSet::new();
    for (&id, gender) in &group.genders {
        let id = u32::try_from(id).map_err(|_| Error::Invalid("negative gender"))?;
        genders.push(prepare_gender(id, gender, input.palettes)?);
        for choices in [
            &gender.headgear,
            &gender.shirts,
            &gender.pants,
            &gender.footwear,
        ] {
            for item in choices {
                needed.insert(item.weenie);
                wardrobe.insert(item.weenie);
            }
        }
    }
    for item in gear {
        if item.heritage.is_none_or(|h| h == heritage) {
            needed.insert(item.template);
        }
    }
    if needed.len() > 4096 {
        return Err(Error::Capacity);
    }
    let mut items = Vec::with_capacity(needed.len());
    for id in needed {
        let value = input.items.get(&id).ok_or(Error::Template(id))?;
        if value.revision == 0 || value.template.weenie_id != id {
            return Err(Error::Invalid("item template revision or identity"));
        }
        value
            .template
            .validate(Default::default())
            .map_err(|_| Error::Invalid("item template"))?;
        let clothing_icons =
            if wardrobe.contains(&id) {
                let table = value
                    .template
                    .properties
                    .data_ids
                    .iter()
                    .find(|p| p.id == 7)
                    .map(|p| p.value)
                    .ok_or(Error::Clothing {
                        template: id,
                        table: 0,
                    })?;
                let decoded = input.clothing.get(&table).filter(|t| t.id == table).ok_or(
                    Error::Clothing {
                        template: id,
                        table,
                    },
                )?;
                if decoded.templates.len() > 4096 {
                    return Err(Error::Capacity);
                }
                decoded
                    .templates
                    .iter()
                    .map(|(&color, value)| (color, value.icon))
                    .collect()
            } else {
                Vec::new()
            };
        items.push(StarterItem {
            template: value.template.clone(),
            revision: value.revision,
            clothing_icons,
        });
    }
    let mut prepared_starts = Vec::with_capacity(starts.len());
    let mut seen = BTreeSet::new();
    for start in starts {
        if !seen.insert(start.area) {
            return Err(Error::Invalid("duplicate starter area"));
        }
        let position = base
            .char_gen()
            .starter_areas
            .get(start.area as usize)
            .and_then(|area| area.locations.first())
            .ok_or(Error::Start(start.area))?;
        prepared_starts.push(CharacterStart {
            area: start.area,
            location: Position {
                obj_cell_id: position.cell,
                position_x: position.origin[0],
                position_y: position.origin[1],
                position_z: position.origin[2],
                rotation_w: position.orientation_wxyz[0],
                rotation_x: position.orientation_wxyz[1],
                rotation_y: position.orientation_wxyz[2],
                rotation_z: position.orientation_wxyz[3],
            },
            instantiation: start.instantiation.clone(),
        });
    }
    Ok(PreparedCreationAssets {
        heritage,
        heritage_name: group.name.clone(),
        rules: (*rules).clone(),
        genders,
        templates: group
            .templates
            .iter()
            .map(|t| CharacterTemplate {
                name: t.name.clone(),
                title: t.title,
            })
            .collect(),
        starts: prepared_starts,
        human: input.human.template.clone(),
        human_revision: input.human.revision,
        items,
        skill_gear: gear.to_vec(),
        skill_spells: spells.to_vec(),
        vital_formulas: [
            formula(&input.vitals.health)?,
            formula(&input.vitals.stamina)?,
            formula(&input.vitals.mana)?,
        ],
    })
}
fn formula(value: &SkillFormula) -> Result<VitalFormula, CreationPreparationError> {
    // CreatureVital.Base uses X as enable and Attr1/Attr2 divided by Z. W/Y
    // remain retained in the decoded VitalTable; this maps that exact consumer.
    if value.x != 0
        && (value.z == 0 || !(1..=6).contains(&value.attribute1) || value.attribute2 > 6)
    {
        return Err(CreationPreparationError::Invalid("vital formula"));
    }
    Ok(VitalFormula {
        enabled: value.x != 0,
        divisor: value.z,
        attribute1: value.attribute1,
        attribute2: value.attribute2,
    })
}
fn palettes(
    id: u32,
    values: &BTreeMap<u32, DatPaletteSet>,
) -> Result<Vec<u32>, CreationPreparationError> {
    let value = values
        .get(&id)
        .filter(|p| p.id == id)
        .ok_or(CreationPreparationError::Palette(id))?;
    if value.palettes.is_empty()
        || value.palettes.len() > 4096
        || value.palettes.iter().any(|id| id >> 24 != 4)
    {
        return Err(CreationPreparationError::Palette(id));
    }
    Ok(value.palettes.clone())
}
fn face(
    gender: u32,
    family: &'static str,
    index: usize,
    description: &ObjectDescription,
) -> Result<FaceTextures, CreationPreparationError> {
    let value = description
        .texture_changes
        .first()
        .ok_or(CreationPreparationError::Face {
            gender,
            family,
            index,
        })?;
    Ok(FaceTextures {
        old: value.old_texture,
        new: value.new_texture,
    })
}
fn prepare_gender(
    id: u32,
    g: &CreationGender,
    sets: &BTreeMap<u32, DatPaletteSet>,
) -> Result<PreparedGender, CreationPreparationError> {
    if [
        g.hair_styles.len(),
        g.hair_colors.len(),
        g.eye_colors.len(),
        g.eyes.len(),
        g.noses.len(),
        g.mouths.len(),
        g.headgear.len(),
        g.shirts.len(),
        g.pants.len(),
        g.footwear.len(),
        g.clothing_colors.len(),
    ]
    .iter()
    .any(|n| *n > 4096)
    {
        return Err(CreationPreparationError::Capacity);
    }
    let hair = g
        .hair_styles
        .iter()
        .map(|h| PreparedHair {
            bald: h.bald,
            alternate_setup: h.alternate_setup,
            head_object: if h.appearance.animation_parts.len() == 1 {
                Some(h.appearance.animation_parts[0].id)
            } else {
                None
            },
            texture: h.appearance.texture_changes.first().map(|t| FaceTextures {
                old: t.old_texture,
                new: t.new_texture,
            }),
            multiple_parts: h.appearance.animation_parts.len() > 1,
        })
        .collect();
    Ok(PreparedGender {
        gender: id,
        scale: g.scale,
        setup: g.setup,
        motion: g.motion_table,
        sound: g.sound_table,
        physics: g.physics_table,
        combat: g.combat_table,
        palette_base: g.base_palette,
        hair,
        hair_palette_sets: g
            .hair_colors
            .iter()
            .map(|id| palettes(*id, sets))
            .collect::<Result<_, _>>()?,
        skin_palettes: palettes(g.skin_palette_set, sets)?,
        eye_palettes: g.eye_colors.clone(),
        eyes: g
            .eyes
            .iter()
            .enumerate()
            .map(|(i, e)| {
                Ok(PreparedEyes {
                    normal: face(id, "eyes", i, &e.appearance)?,
                    bald: face(id, "bald eyes", i, &e.bald_appearance)?,
                })
            })
            .collect::<Result<_, CreationPreparationError>>()?,
        noses: g
            .noses
            .iter()
            .enumerate()
            .map(|(i, n)| face(id, "nose", i, &n.appearance))
            .collect::<Result<_, _>>()?,
        mouths: g
            .mouths
            .iter()
            .enumerate()
            .map(|(i, m)| face(id, "mouth", i, &m.appearance))
            .collect::<Result<_, _>>()?,
        clothing: [&g.headgear, &g.shirts, &g.pants, &g.footwear]
            .map(|list| list.iter().map(|i| i.weenie).collect()),
        clothing_colors: g.clothing_colors.clone(),
    })
}
