//! Reconstruct one live source's immutable assets from its exact accepted manifest.
use super::*;
use crate::npc_region::PreparedNpcRegionSource;
use bace_storage_codec::{ItemPlacementV2, ItemSaveV5, PackKey, PackLookup};
impl VerifiedRegionAssets {
    pub(crate) fn prepare_pinned_npc_region(
        &mut self,
        base: &PreparedRegionActivation,
        request: &RegionActivationRequest,
        work: &crate::npc_region::NpcRegionRequest,
        directory: &std::path::Path,
    ) -> Result<PreparedNpcRegionSource, String> {
        let pin = &work.source;
        let frozen = bace_storage_codec::NpcWorkflowSaveV3::decode_or_migrate(&pin.head.checkpoint)
            .map_err(|e| e.to_string())?;
        if frozen.archive.is_some()
            || frozen.source != pin.head.source
            || frozen.source_version != pin.head.source_version
            || frozen.content_generation != pin.manifest_hash
            || pin
                .manifest
                .content_hash(Default::default())
                .map_err(|e| e.to_string())?
                != pin.manifest_hash
        {
            return Err("NPC regional source head mismatch".into());
        }
        let frozen_location = frozen
            .location
            .clone()
            .ok_or("legacy NPC head requires proven live source location")?;
        if frozen_location.player || frozen_location.cell >> 16 != u32::from(base.fence.landblock) {
            return Err("NPC regional source location mismatch".into());
        }
        let generation = Arc::new(
            pin.manifest
                .open(directory, Default::default())
                .map_err(|e| e.to_string())?,
        );
        let mut source =
            crate::npc_recovery::prepare_npc_source(&generation, frozen.source_template)?;
        if source.program_hash != frozen.program_hash {
            return Err("NPC regional source program mismatch".into());
        }
        let binding = crate::npc_persistence::NpcCheckpointBinding {
            source_version: frozen.source_version,
            source_template: frozen.source_template,
            source: frozen.source,
            invocation: frozen.invocation,
            program_hash: frozen.program_hash,
            content_generation: frozen.content_generation,
        };
        let inventory = frozen.inventory.clone();
        if let Some(origin) = inventory.as_ref().and_then(|i| i.origin.as_ref()) {
            // Static authored creatures carry a self-origin token from region
            // admission. Only the pinned authored Shop may reuse that token;
            // a child of a real generator still needs parent membership.
            if source.authored.weenie_type != 12
                || origin.generator != frozen.source
                || origin.profile != 0
                || origin.incarnation != origin.child_incarnation
                || origin.content_revision != generation.revision()
            {
                return Err(
                    "NPC generated-source recovery requires exact parent membership admission"
                        .into(),
                );
            }
        }
        let snapshot = crate::npc_persistence::restore_checkpoint(binding, frozen)
            .map_err(|e| e.to_string())?;
        let location = snapshot.location.ok_or("NPC source location missing")?;
        if let Some(properties) = snapshot.properties {
            source.properties = properties;
        }
        let authored = Arc::new(crate::npc_recovery::overlay_npc_qualities(
            &source.authored,
            &source.properties,
        )?);
        if !authored.properties.generators.is_empty() {
            return Err(
                "historical NPC generator catalog requires accepted generator-generation routing"
                    .into(),
            );
        }
        if authored.weenie_type == 12 {
            verify_static_shop_source(
                work.baseline.as_ref(),
                &frozen_location,
                &generation,
                &authored,
                pin.head.source,
                base.fence.landblock,
            )?;
            if inventory.is_none() {
                return Err("historical static Shop inventory proof missing".into());
            }
        }
        let roots = BTreeMap::from([(source.template, authored.clone())]);
        let catalog = crate::generator_catalog::prepare_generator_catalog(
            &generation,
            &roots,
            request
                .treasure_assets
                .clone()
                .ok_or("NPC treasure assets missing")?,
            request.aetheria_drop_rate,
        )?;
        let item_spells = crate::generator_spell_assets::prepare_generator_spell_assets(
            &generation,
            &mut self.portal,
            &catalog.templates,
        );
        let mut physical = BTreeMap::new();
        for (&id, template) in &catalog.templates {
            physical.insert(id, self.prepare_template(template));
        }
        let mut creatures = BTreeMap::new();
        if location.facts.creature {
            let shape = physical
                .get(&source.template)
                .ok_or("NPC physical source missing")?
                .as_ref()
                .map_err(Clone::clone)?;
            let mut creature = self.prepare_creature(
                &authored,
                shape,
                request
                    .creature_policy
                    .ok_or("NPC creature policy missing")?,
            )?;
            let mut historical_request = request.clone();
            historical_request.generation = generation.clone();
            historical_request.content_hash = pin.manifest_hash;
            creature.ace_loot = Some(prepare_ace_loot(
                &historical_request,
                &authored,
                &catalog.templates,
                &catalog,
            )?);
            creatures.insert(source.template, Ok(creature));
        }
        let instance = crate::npc_region::accepted_instance(pin.head.source, &authored, location);
        let content = crate::world_content::PreparedRegion {
            templates: catalog.templates.clone(),
            content_generation: generation.revision(),
            landblock: base.fence.landblock,
            instances: vec![instance],
            encounters: vec![],
        };
        let prepared = Arc::new(PreparedRegionActivation {
            source_manifest: pin.manifest_hash,
            npc_recovery: BTreeMap::new(),
            generation: generation.clone(),
            visibility: base.visibility.clone(),
            fence: RegionActivationFence {
                content_generation: generation.revision(),
                ..base.fence
            },
            content,
            catalog,
            item_spells,
            geometry: base.geometry.clone(),
            physical,
            creatures,
        });
        let radius = match source
            .properties
            .get(bace_entity::PropertyFamily::Float, 54)
        {
            Some(bace_entity::PropertyValue::Float(v)) => *v as f32,
            _ => 0.6,
        };
        // The archived source keeps its signed authored Float54. ACE applies
        // the raw threshold to signed cylinder distance; only absent Float54
        // receives the 0.6 default.
        if !radius.is_finite() {
            return Err("NPC source use radius".into());
        }
        let registration = crate::npc_sources::PreparedNpcRegistration {
            admitted: false,
            object_registry: crate::npc_region::restore_object_registry(
                location,
                &prepared,
                inventory.as_ref(),
            )?,
            baseline: work.baseline.clone(),
            actor: bace_types::EntityId(pin.head.source),
            landblock: base.fence.landblock,
            epoch: base.fence.activation_epoch,
            content_hash: pin.manifest_hash,
            generation,
            source: Arc::new(source),
            use_radius: radius,
        };
        Ok(PreparedNpcRegionSource {
            inventory,
            items: Arc::new(work.inventory.clone()),
            registration,
            location,
            prepared,
        })
    }
}

/// An authored Shop's V5 root is the durable identity of this pinned NPC.
/// Generic Creature restoration must never create a second actor for it.
fn verify_static_shop_source(
    baseline: Option<&bace_persistence::StoredAggregate>,
    location: &bace_storage_codec::npc_workflow_v3::NpcSourceLocationV3,
    generation: &PackGeneration,
    source: &bace_content::WeenieV1,
    actor: u32,
    landblock: u16,
) -> Result<(), String> {
    let baseline = baseline.ok_or("historical static Shop V5 source missing")?;
    if baseline.object_id != actor || baseline.persisted_version == 0 || !location.creature {
        return Err("historical static Shop source identity".into());
    }
    let saved = ItemSaveV5::decode(&baseline.bytes).map_err(|e| e.to_string())?;
    let ItemPlacementV2::World(position) = &saved.placement else {
        return Err("historical static Shop world placement".into());
    };
    let (sin, cos) = (location.heading * 0.5).sin_cos();
    if saved.entity.object_id != actor
        || saved.entity.state.weenie_id != source.weenie_id
        || saved.entity.state != *source
        || saved.entity.state.weenie_type != 12
        || saved.entity.template_revision != generation.revision()
        || saved.entity.mutation_revision == 0
        || saved.construction.is_some()
        || saved.source_destination.is_some()
        || position.obj_cell_id != location.cell
        || position.position_x != location.position[0]
        || position.position_y != location.position[1]
        || position.position_z != location.position[2]
        || position.rotation_w != cos
        || position.rotation_x != 0.0
        || position.rotation_y != 0.0
        || position.rotation_z != sin
    {
        return Err("historical static Shop V5 source fence".into());
    }
    match generation
        .lookup(PackKey {
            namespace: 20,
            id: u64::from(actor),
        })
        .map_err(|e| e.to_string())?
    {
        PackLookup::Record(record) => match bace_content_tools::decode_world_record(record.bytes())
            .map_err(|e| e.to_string())?
        {
            bace_content::WorldRecordV1::LandblockInstance(instance)
                if instance.guid == actor
                    && !instance.is_link_child
                    && instance.landblock == i32::from(landblock)
                    && instance.obj_cell_id == location.cell
                    && instance.weenie_class_id == source.weenie_id =>
            {
                Ok(())
            }
            _ => Err("historical static Shop authored instance mismatch".into()),
        },
        _ => Err("historical static Shop authored instance missing".into()),
    }
}
