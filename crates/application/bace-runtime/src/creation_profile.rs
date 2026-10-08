//! Pinned binary bootstrap defaults joined to accepted native rows and DAT starts.
//! This module performs no asset I/O and does not grant geometry admission.
use bace_content::{CharacterStartProfileV1, Position, SpellRowV1};
/// Only this compact derived index is decoded; no startup scan of world records.
pub fn load_creature_names(
    generation: &bace_storage_codec::PackGeneration,
) -> Result<bace_content::CreatureNameIndexV1, String> {
    let bace_storage_codec::PackLookup::Record(record) = generation
        .lookup(bace_storage_codec::PackKey {
            namespace: 48,
            id: 1,
        })
        .map_err(|e| e.to_string())?
    else {
        return Err(
            "accepted pack lacks character-name index; rebuild/publish with current world compiler"
                .into(),
        );
    };
    bace_content_tools::decode_creature_names(record.bytes())
}
pub fn builtin_creation_profile() -> Result<CharacterStartProfileV1, String> {
    bace_content_tools::decode_character_start(include_bytes!("../data/character-start.bace"))
}
pub fn starter_rules(
    profile: &CharacterStartProfileV1,
) -> Result<
    (
        Vec<bace_character::SkillGear>,
        Vec<bace_character::SkillSpell>,
    ),
    String,
> {
    profile.validate()?;
    Ok((
        profile
            .gear
            .iter()
            .map(|g| bace_character::SkillGear {
                skill: g.skill,
                heritage: g.heritage,
                template: g.template,
                count: g.count,
            })
            .collect(),
        profile
            .spells
            .iter()
            .map(|s| bace_character::SkillSpell {
                skill: s.skill,
                spell: s.spell,
                specialized_only: s.specialized_only,
            })
            .collect(),
    ))
}
pub fn prepare_start_areas(
    profile: &CharacterStartProfileV1,
    areas: &[bace_dat::StarterArea],
    mut spell: impl FnMut(u32) -> Option<SpellRowV1>,
) -> Result<Vec<crate::character_preparation::AdmittedStart>, String> {
    profile.validate()?;
    if areas.is_empty() || areas.len() > 256 {
        return Err("character start-area capacity".into());
    }
    areas
        .iter()
        .enumerate()
        .map(|(index, area)| {
            let location = area
                .locations
                .first()
                .ok_or("character start area has no DAT location")?;
            let rule = profile.starts.iter().find(|r| r.name == area.name);
            let spell_id = rule.map_or(Some(profile.default_start_spell), |r| r.free_ride_spell);
            let instantiation = if let Some(id) = spell_id {
                let row = spell(id)
                    .filter(|s| s.id == id)
                    .ok_or("missing accepted start-area spell")?;
                Position {
                    obj_cell_id: row.position_obj_cell_id.ok_or("start spell cell")?,
                    position_x: row.position_origin_x.ok_or("start spell x")?,
                    position_y: row.position_origin_y.ok_or("start spell y")?,
                    position_z: row.position_origin_z.ok_or("start spell z")?,
                    rotation_w: row.position_angles_w.ok_or("start spell w")?,
                    rotation_x: row.position_angles_x.ok_or("start spell x rotation")?,
                    rotation_y: row.position_angles_y.ok_or("start spell y rotation")?,
                    rotation_z: row.position_angles_z.ok_or("start spell z rotation")?,
                }
            } else {
                Position {
                    obj_cell_id: location.cell,
                    position_x: location.origin[0],
                    position_y: location.origin[1],
                    position_z: location.origin[2],
                    rotation_w: location.orientation_wxyz[0],
                    rotation_x: location.orientation_wxyz[1],
                    rotation_y: location.orientation_wxyz[2],
                    rotation_z: location.orientation_wxyz[3],
                }
            };
            bace_interactions::PortalPosition {
                cell: instantiation.obj_cell_id,
                origin: [
                    instantiation.position_x,
                    instantiation.position_y,
                    instantiation.position_z,
                ],
                rotation: [
                    instantiation.rotation_w,
                    instantiation.rotation_x,
                    instantiation.rotation_y,
                    instantiation.rotation_z,
                ],
            }
            .validate()
            .map_err(|e| format!("invalid start spell position: {e:?}"))?;
            Ok(crate::character_preparation::AdmittedStart {
                area: index as u32,
                instantiation,
            })
        })
        .collect()
}
