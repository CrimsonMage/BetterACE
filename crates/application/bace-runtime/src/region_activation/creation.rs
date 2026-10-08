//! Referenced creation-only DAT/native closure. No scan or decoding of the world.
use super::*;
use bace_content::{CharacterStartProfileV1, WorldRecordV1};
use bace_storage_codec::{PackKey, PackLookup};
impl VerifiedRegionAssets {
    pub fn prepare_creation_assets(
        &mut self,
        generation: &Arc<PackGeneration>,
        heritage: u32,
        profile: &CharacterStartProfileV1,
    ) -> Result<crate::creation_assets::PreparedCreationClosure, String> {
        profile.validate()?;
        if generation.revision() == 0 || !(1..=11).contains(&heritage) {
            return Err("unsupported creation generation or heritage".into());
        }
        let mut budget = 64 * 1024 * 1024usize;
        let xp = bace_dat::XpTable::decode(&read(
            &mut self.portal,
            bace_dat::XpTable::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let skills = bace_dat::SkillTable::decode(&read(
            &mut self.portal,
            bace_dat::SkillTable::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let chargen = bace_dat::CharGen::decode(&read(
            &mut self.portal,
            bace_dat::CharGen::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let vitals =
            bace_dat::VitalTable::decode(&read(&mut self.portal, 0x0e000003, &mut budget)?)
                .map_err(|e| e.to_string())?;
        let taboo = bace_dat::TabooTable::decode(&read(
            &mut self.portal,
            bace_dat::TabooTable::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let name_index = crate::creation_profile::load_creature_names(generation)?;
        let name_bytes = name_index
            .entries
            .iter()
            .try_fold(0usize, |sum, e| sum.checked_add(e.name.len()))
            .ok_or("creation name byte budget")?;
        budget = budget
            .checked_sub(name_bytes)
            .ok_or("creation asset byte budget")?;
        let names: Vec<_> = name_index
            .entries
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        let names = Arc::new(
            crate::name_preparation::prepare_name_policy(&taboo, &names)
                .map_err(|e| format!("creation name policy: {e:?}"))?,
        );
        let character = Arc::new(
            crate::character_assets::prepare_character_assets(xp, skills, chargen)
                .map_err(|e| e.to_string())?,
        );
        let group = character
            .char_gen()
            .heritage_groups
            .get(&heritage)
            .ok_or("missing creation heritage")?;
        if group.genders.len() > 8 {
            return Err("creation gender capacity".into());
        }
        let mut palette_ids = BTreeSet::new();
        let mut template_ids = BTreeSet::new();
        let mut wardrobe = BTreeSet::new();
        template_ids.insert(profile.human_template);
        for gender in group.genders.values() {
            palette_ids.insert(gender.skin_palette_set);
            palette_ids.extend(gender.hair_colors.iter().copied());
            for list in [
                &gender.headgear,
                &gender.shirts,
                &gender.pants,
                &gender.footwear,
            ] {
                for item in list {
                    template_ids.insert(item.weenie);
                    wardrobe.insert(item.weenie);
                }
            }
        }
        for item in &profile.gear {
            if item.heritage.is_none_or(|id| id == heritage) {
                template_ids.insert(item.template);
            }
        }
        if palette_ids.len() > 4096 || template_ids.len() > 4096 {
            return Err("creation referenced closure capacity".into());
        }
        let mut palettes = BTreeMap::new();
        for id in palette_ids {
            if id >> 24 != 0x0f {
                return Err("invalid creation palette-set reference".into());
            }
            let value = bace_dat::DatPaletteSet::decode(&read(&mut self.portal, id, &mut budget)?)
                .map_err(|e| e.to_string())?;
            if value.id != id {
                return Err("creation palette-set identity".into());
            }
            palettes.insert(id, value);
        }
        let mut templates = BTreeMap::new();
        let mut clothing = BTreeMap::new();
        for id in template_ids {
            let PackLookup::Record(record) = generation
                .lookup(PackKey {
                    namespace: 1,
                    id: u64::from(id),
                })
                .map_err(|e| e.to_string())?
            else {
                return Err(format!("missing accepted creation template {id}"));
            };
            budget = budget
                .checked_sub(record.bytes().len())
                .ok_or("creation template byte budget")?;
            let template = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
            if template.weenie_id != id {
                return Err("creation template index identity".into());
            }
            if wardrobe.contains(&id) {
                let did = template
                    .properties
                    .data_ids
                    .iter()
                    .find(|p| p.id == 7)
                    .map(|p| p.value)
                    .ok_or_else(|| format!("starting clothing {id} has no DAT table"))?;
                if did >> 24 != 0x10 {
                    return Err("invalid starting clothing DAT reference".into());
                }
                if let std::collections::btree_map::Entry::Vacant(entry) = clothing.entry(did) {
                    let value =
                        bace_dat::ClothingTable::decode(&read(&mut self.portal, did, &mut budget)?)
                            .map_err(|e| e.to_string())?;
                    if value.id != did {
                        return Err("creation clothing table identity".into());
                    }
                    entry.insert(value);
                }
            }
            templates.insert(
                id,
                crate::character_preparation::CreationTemplateInput {
                    revision: generation.revision(),
                    template,
                },
            );
        }
        let human = templates
            .remove(&profile.human_template)
            .ok_or("missing human creation template")?;
        let mut start_spells = BTreeSet::from([profile.default_start_spell]);
        start_spells.extend(profile.starts.iter().filter_map(|s| s.free_ride_spell));
        let mut spell_rows = BTreeMap::new();
        for id in start_spells {
            let PackLookup::Record(record) = generation
                .lookup(PackKey {
                    namespace: 38,
                    id: u64::from(id),
                })
                .map_err(|e| e.to_string())?
            else {
                return Err(format!("missing accepted creation start spell {id}"));
            };
            budget = budget
                .checked_sub(record.bytes().len())
                .ok_or("creation start byte budget")?;
            let WorldRecordV1::Spell(spell) =
                bace_content_tools::decode_world_record(record.bytes())?
            else {
                return Err("creation start namespace mismatch".into());
            };
            if spell.id != id {
                return Err("creation start spell identity".into());
            }
            spell_rows.insert(id, *spell);
        }
        let starts = crate::creation_profile::prepare_start_areas(
            profile,
            &character.char_gen().starter_areas,
            |id| spell_rows.get(&id).cloned(),
        )?;
        let (gear, spells) = crate::creation_profile::starter_rules(profile)?;
        let spell_table = bace_dat::SpellTable::decode(&read(
            &mut self.portal,
            bace_dat::SpellTable::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        if spells
            .iter()
            .any(|s| !spell_table.spells.contains_key(&s.spell))
            || human.template.properties.spell_book.iter().any(|s| {
                u32::try_from(s.id)
                    .ok()
                    .is_none_or(|id| !spell_table.spells.contains_key(&id))
            })
        {
            return Err("creation grants an unavailable DAT spell".into());
        }
        let creation = crate::character_preparation::prepare_creation_assets(
            &character,
            heritage,
            crate::character_preparation::CreationSupplementary {
                vitals: &vitals,
                palettes: &palettes,
                clothing: &clothing,
                human: &human,
                items: &templates,
                skill_gear: Some(&gear),
                skill_spells: Some(&spells),
                starts: Some(&starts),
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(crate::creation_assets::PreparedCreationClosure {
            content_generation: generation.revision(),
            character,
            creation: Arc::new(creation),
            names,
        })
    }
    /// Static placement uses the same checked geometry gate as Body::spawn_geometry;
    /// live actor ownership/entry is still admitted by the simulation after save.
    pub fn validate_creation_geometry(
        &mut self,
        prepared: &bace_character::PreparedCharacter,
    ) -> Result<Arc<GeometryRegion>, String> {
        let source = &prepared.state;
        let position = &source
            .properties
            .positions
            .iter()
            .find(|p| p.id == 1)
            .ok_or("creation has no source location")?
            .value;
        if position.rotation_x != 0. || position.rotation_y != 0. {
            return Err("unsupported created player root orientation".into());
        }
        let geometry = self.prepare_geometry((position.obj_cell_id >> 16) as u16)?;
        let setup = source
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 1)
            .map(|p| p.value)
            .ok_or("creation has no collision setup")?;
        let mut budget = 16 * 1024 * 1024usize;
        let setup = CollisionSetup::decode(&read(&mut self.portal, setup, &mut budget)?)
            .map_err(|e| e.to_string())?;
        let scale = source
            .properties
            .floats
            .iter()
            .find(|p| p.id == 39)
            .map_or(1., |p| p.value) as f32;
        let shape = crate::world_admission::prepare_collision_shape(&setup, scale)?;
        geometry
            .validate_placement(
                position.obj_cell_id,
                bace_geometry::Vec3::new(
                    position.position_x,
                    position.position_y,
                    position.position_z,
                ),
                &shape,
                &[],
                0,
            )
            .map_err(|e| format!("creation collision admission: {e:?}"))?;
        Ok(geometry)
    }
}
