//! Reconstruct one live source's immutable assets from its exact accepted manifest.
use super::*;
use crate::npc_region::PreparedNpcRegionSource;
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
            .as_ref()
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
        if inventory.as_ref().is_some_and(|i| i.origin.is_some()) {
            return Err(
                "NPC generated-source recovery requires exact parent membership admission".into(),
            );
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
        if !authored.properties.generators.is_empty() || authored.weenie_type == 12 {
            return Err(
                "historical NPC generator catalog requires accepted generator-generation routing"
                    .into(),
            );
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
