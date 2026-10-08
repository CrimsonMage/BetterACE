//! Startup reads indexed spell rows and the pinned treasure dependency closure,
//! never all world templates. Run exclusively on bounded cold preparation capacity.
use super::*;
use bace_storage_codec::{PackKey, PackLookup};
impl VerifiedRegionAssets {
    pub fn prepare_runtime_startup_assets(
        &mut self,
        generation: &Arc<PackGeneration>,
        policy: crate::startup_assets::StartupAssetPolicy,
    ) -> Result<crate::game_runtime::GameRuntimeStartupAssets, String> {
        if generation.revision() == 0
            || policy.treasure_table_set_id == 0
            || policy.creature_think_interval == 0
            || policy.creature_corpse_decay_ticks == 0
            || policy.creature_think_interval > 30 * 60
            || policy.creature_corpse_decay_ticks > 30 * 86400
            || !policy.aetheria_drop_rate.is_finite()
            || policy.aetheria_drop_rate < 0.
            || !(-86400..=86400).contains(&policy.local_offset_seconds)
        {
            return Err("invalid startup asset policy".into());
        }
        let PackLookup::Record(table_record) = generation
            .lookup(PackKey {
                namespace: 52,
                id: u64::from(policy.treasure_table_set_id),
            })
            .map_err(|e| e.to_string())?
        else {
            return Err("accepted treasure table set missing".into());
        };
        if table_record.schema() != 1 {
            return Err("unsupported treasure table set schema".into());
        }
        let table_set = bace_content_tools::decode_treasure_table_set(table_record.bytes())?;
        if table_set.id != policy.treasure_table_set_id {
            return Err("treasure table set identity mismatch".into());
        }
        bace_loot::ace_tables::install(table_set)?;
        let PackLookup::Record(classes) = generation
            .lookup(PackKey {
                namespace: 49,
                id: 1,
            })
            .map_err(|e| e.to_string())?
        else {
            return Err("startup requires accepted class-name index".into());
        };
        let classes = bace_content_tools::decode_template_classes(classes.bytes())?;
        let corpse_template = classes
            .entries
            .iter()
            .find(|entry| entry.class_name == "corpse")
            .ok_or("accepted corpse class missing")?
            .template;
        let mut budget = 64 * 1024 * 1024usize;
        let mut rows = BTreeMap::new();
        let mut cursor = Some(PackKey {
            namespace: 37,
            id: u64::MAX,
        });
        'rows: loop {
            let batch = generation.scan(cursor, 128).map_err(|e| e.to_string())?;
            if batch.is_empty() {
                break;
            }
            for (key, value) in batch {
                cursor = Some(key);
                if key.namespace != 38 {
                    break 'rows;
                }
                let PackLookup::Record(record) = value else {
                    continue;
                };
                budget = budget
                    .checked_sub(record.bytes().len())
                    .ok_or("startup spell byte capacity")?;
                let bace_content::WorldRecordV1::Spell(row) =
                    bace_content_tools::decode_world_record(record.bytes())?
                else {
                    return Err("startup spell namespace mismatch".into());
                };
                if record.schema() != 1 || u64::from(row.id) != key.id || rows.len() >= 16384 {
                    return Err("startup spell identity/capacity".into());
                }
                rows.insert(row.id, *row);
            }
        }
        let spells = bace_dat::SpellTable::decode(&read(
            &mut self.portal,
            bace_dat::SpellTable::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let staff_magic = Arc::new(crate::staff_magic_assets::prepare_staff_magic_assets(
            &spells, &rows,
        )?);
        let xp = bace_dat::XpTable::decode(&read(
            &mut self.portal,
            bace_dat::XpTable::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let levels = Arc::new(
            bace_character::CharacterLevelTable::prepare(
                xp.character_level_xp,
                xp.character_level_skill_credits,
            )
            .map_err(|e| format!("startup level table: {e:?}"))?,
        );
        let mapper = bace_dat::DualDidMapper::decode(&read(
            &mut self.portal,
            bace_dat::DualDidMapper::COMPONENT_RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let component_templates: BTreeSet<_> = mapper
            .client_ids
            .values()
            .copied()
            .filter(|id| *id != 0)
            .collect();
        if component_templates.is_empty() || component_templates.len() > 4096 {
            return Err("startup component template capacity".into());
        }
        let projectile_ids: BTreeSet<_> = rows
            .values()
            .filter_map(|row| row.wcid)
            .filter(|id| *id != 0)
            .collect();
        if projectile_ids.len() > 1024 {
            return Err("startup projectile shape capacity".into());
        }
        let mut projectile_shapes = BTreeMap::new();
        let mut projectile_sources = Vec::new();
        for id in projectile_ids {
            let PackLookup::Record(record) = generation
                .lookup(PackKey {
                    namespace: 1,
                    id: u64::from(id),
                })
                .map_err(|e| e.to_string())?
            else {
                return Err(format!("startup spell template {id} missing"));
            };
            budget = budget
                .checked_sub(record.bytes().len())
                .ok_or("startup projectile byte capacity")?;
            let template = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
            if template.weenie_id != id {
                return Err("startup projectile template identity".into());
            }
            projectile_shapes.insert(id, self.prepare_world_item_shape(&template)?);
            projectile_sources.push(Arc::new(template));
        }
        let projectile_visibility =
            self.prepare_projectile_visibility_templates(&projectile_sources)?;
        let template_ids = bace_loot::pinned_treasure_templates()
            .map_err(|e| format!("treasure source closure: {e:?}"))?;
        let header = self.portal.header();
        let version = bace_dat::DatTableVersion {
            engine_version: header.engine_version,
            game_version: header.game_version,
            record_iteration: self
                .portal
                .records()
                .get(&bace_dat::SpellTable::RECORD_ID)
                .ok_or("startup spell record missing")?
                .iteration,
        };
        let treasure = crate::treasure_assets::prepare_treasure_assets(
            generation,
            &template_ids,
            &mut self.portal,
            version,
        )?;
        let creatures = crate::generator_preparation::CreatureAdmissionPolicy {
            corpse_template,
            think_interval: policy.creature_think_interval,
            corpse_decay_ticks: policy.creature_corpse_decay_ticks,
        };
        Ok(crate::game_runtime::GameRuntimeStartupAssets {
            runtime: crate::game_runtime::GameRuntimeAssets {
                world_open: policy.world_open,
                local_offset_seconds: policy.local_offset_seconds,
                staff_definitions: staff_magic.definitions.clone().into(),
                spell_rows: Arc::new(rows),
                component_templates: component_templates.into_iter().collect::<Vec<_>>().into(),
                projectile_shapes: Arc::new(projectile_shapes),
                projectile_visibility: Arc::new(projectile_visibility),
                creatures,
                policy: policy.player,
            },
            recall_locations: Arc::new(crate::portal_preparation::prepare_recall_locations(
                generation, &classes,
            )?),
            staff_magic,
            levels,
            treasure,
            creatures,
            generators: Default::default(),
            aetheria_drop_rate: policy.aetheria_drop_rate,
        })
    }
}
