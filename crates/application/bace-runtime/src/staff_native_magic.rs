//! Cold, actor-bound native casting programs. Account formulas never become a
//! globally reusable casting program; source spell semantics remain shared.
use crate::{
    magic_preparation::prepare_component_plan,
    native_magic_assets::{NativeMagicPreparation, prepare_native_magic_spell},
    region_activation::RegionAssetManifest,
    world_admission::MotionChainRequest,
};
use bace_content::{SpellRowV1, WeenieV1};
use bace_dat::{
    Animation, CollisionSetup, DatArchive, DualDidMapper, MotionTable, SpellComponents, SpellTable,
};
use bace_gameplay_api::CharacterBinding;
use bace_storage_codec::{PackGeneration, PackKey, PackLookup};
use std::{collections::BTreeMap, sync::Arc};
mod portals;
mod worker;
pub use worker::StaffNativeMagicWorker;
#[derive(Clone)]
pub struct StaffNativeMagicWork {
    pub token: u64,
    pub binding: CharacterBinding,
    pub expected_character_revision: u64,
    pub source: Arc<WeenieV1>,
    pub account_cp1252: Vec<u8>,
    pub top_level_templates: Vec<u32>,
    pub row: Arc<SpellRowV1>,
}
pub struct StaffNativeMagicResult {
    pub work: Arc<StaffNativeMagicWork>,
    pub result: Result<bace_simulation::PreparedActorMagicProgram, String>,
}
pub struct StaffNativeMagicAssets {
    portal: DatArchive,
    spells: SpellTable,
    components: SpellComponents,
    mapper: DualDidMapper,
    generation: Arc<PackGeneration>,
    motions: Option<(u32, MotionTable, BTreeMap<u32, Animation>)>,
}
impl StaffNativeMagicAssets {
    pub fn open(
        manifest: &RegionAssetManifest,
        generation: Arc<PackGeneration>,
    ) -> Result<Self, String> {
        if manifest.portal_sha256.len() != 64
            || bace_dat::fingerprint(&manifest.portal).map_err(|e| e.to_string())?
                != manifest.portal_sha256
        {
            return Err("unapproved native magic DAT fingerprint".into());
        }
        let mut portal = DatArchive::open(&manifest.portal).map_err(|e| e.to_string())?;
        if portal.header().dataset != 1 {
            return Err("native magic DAT dataset".into());
        }
        let mut budget = 64 * 1024 * 1024;
        let spells = SpellTable::decode(&read(&mut portal, SpellTable::RECORD_ID, &mut budget)?)
            .map_err(|e| e.to_string())?;
        let components =
            SpellComponents::decode(&read(&mut portal, SpellComponents::RECORD_ID, &mut budget)?)
                .map_err(|e| e.to_string())?;
        let mapper = DualDidMapper::decode(&read(&mut portal, 0x27000002, &mut budget)?)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            portal,
            spells,
            components,
            mapper,
            generation,
            motions: None,
        })
    }
    pub fn prepare(
        &mut self,
        work: &StaffNativeMagicWork,
    ) -> Result<bace_simulation::PreparedActorMagicProgram, String> {
        if work.token == 0
            || work.binding.actor.0 == 0
            || work.account_cp1252.is_empty()
            || work.account_cp1252.len() > 200
            || work.top_level_templates.len() > 4096
        {
            return Err("native actor program identity/bounds".into());
        }
        let did = |source: &WeenieV1, id| {
            source
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.value)
                .ok_or("native actor DAT reference missing")
        };
        let motion_id = did(&work.source, 2)?;
        let mut budget = 64 * 1024 * 1024;
        if self
            .motions
            .as_ref()
            .is_none_or(|(id, _, _)| *id != motion_id)
        {
            let motions = MotionTable::decode(&read(&mut self.portal, motion_id, &mut budget)?)
                .map_err(|e| e.to_string())?;
            let mut animations = BTreeMap::new();
            for data in motions
                .cycles
                .values()
                .chain(motions.modifiers.values())
                .chain(motions.links.values().flat_map(|v| v.values()))
            {
                for segment in &data.animations {
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        animations.entry(segment.animation_id)
                    {
                        if *entry.key() >> 24 != 3 {
                            return Err("native cast animation namespace".into());
                        }
                        entry.insert(
                            Animation::decode(&read(
                                &mut self.portal,
                                segment.animation_id,
                                &mut budget,
                            )?)
                            .map_err(|e| e.to_string())?,
                        );
                    }
                }
            }
            self.motions = Some((motion_id, motions, animations));
        }
        let base = self
            .spells
            .spells
            .get(&work.row.id)
            .ok_or("native spell DAT entry missing")?;
        let infused = [297, 296, 294, 295, 328].map(|id| {
            work.source
                .properties
                .ints
                .iter()
                .any(|p| p.id == id && p.value > 0)
        });
        let plan = prepare_component_plan(
            base,
            &self.components,
            &self.mapper,
            &work.account_cp1252,
            infused,
            &work.top_level_templates,
        )
        .map_err(|e| format!("native components: {e:?}"))?;
        let projectile = if let Some(id) = work
            .row
            .wcid
            .filter(|id| *id != 0 && matches!(base.meta_type, 2 | 10 | 15))
        {
            let PackLookup::Record(record) = self
                .generation
                .lookup(PackKey {
                    namespace: 1,
                    id: u64::from(id),
                })
                .map_err(|e| e.to_string())?
            else {
                return Err("native projectile template missing".into());
            };
            budget = budget
                .checked_sub(record.bytes().len())
                .ok_or("native magic content byte budget")?;
            let source: WeenieV1 =
                bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
            if source.weenie_id != id {
                return Err("native projectile template identity".into());
            }
            let setup =
                CollisionSetup::decode(&read(&mut self.portal, did(&source, 1)?, &mut budget)?)
                    .map_err(|e| e.to_string())?;
            let shape = crate::world_admission::prepare_collision_shape(&setup, scale(&source)?)?;
            Some((source, shape))
        } else {
            None
        };
        let (_, motions, animations) = self.motions.as_ref().expect("prepared motion closure");
        // GDLE casting enters Motion_Magic, starts from its Ready substate and
        // prepares the source gestures at speed 2, including the Ready suffix.
        let prepared = prepare_native_magic_spell(NativeMagicPreparation {
            id: work.row.id,
            base,
            row: &work.row,
            components: &self.components,
            component_plan: &plan,
            motion_table: motions,
            animations,
            motion_context: MotionChainRequest {
                style: 0x80000049,
                current_motion: 0x41000003,
                current_speed: 1.0,
                action: 0,
                action_speed: 2.0,
                scale: scale(&work.source)?,
                modifiers: &[],
            },
            projectile: projectile
                .as_ref()
                .map(|(source, shape)| (source, shape.clone())),
        })?;
        let (portal_templates, destination_cells) = self.prepare_portals(
            &work.source,
            &prepared.definition.spell.spell.effect,
            &mut budget,
        )?;
        Ok(bace_simulation::PreparedActorMagicProgram {
            portal_templates,
            destination_cells,
            binding: work.binding,
            expected_character_revision: work.expected_character_revision,
            definition: prepared.definition,
            projectile_shapes: prepared.projectile.into_iter().collect(),
        })
    }
}
fn scale(source: &WeenieV1) -> Result<f32, String> {
    let scale = source
        .properties
        .floats
        .iter()
        .find(|p| p.id == 39)
        .map_or(1.0, |p| p.value) as f32;
    if !scale.is_finite() || scale <= 0.0 {
        return Err("native actor scale".into());
    }
    Ok(scale)
}
fn read(portal: &mut DatArchive, id: u32, budget: &mut usize) -> Result<Vec<u8>, String> {
    let size = portal
        .records()
        .get(&id)
        .ok_or("native DAT record missing")?
        .size as usize;
    *budget = budget
        .checked_sub(size)
        .ok_or("native magic asset byte budget")?;
    portal.read(id).map_err(|e| e.to_string())
}
