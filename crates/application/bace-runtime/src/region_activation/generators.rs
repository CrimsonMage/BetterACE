//! Cold source-root adaptation. Failed geometry/profile prerequisites remain
//! explicit held roots; the valid immutable batch is admitted by one owner command.
use super::*;
use bace_gameplay_api::{GeneratorIdentity, GeneratorLocation};
use bace_simulation::{PreparedGeneratorRegion, PreparedGeneratorRoot};
use bace_types::EntityId;
use sha2::{Digest, Sha256};

pub struct PreparedGeneratorActivation {
    pub admission: PreparedGeneratorRegion,
    pub held: Vec<(u32, String)>,
}
impl PreparedRegionActivation {
    /// Encounter IDs are allocated durably before preparation; row IDs are not
    /// object GUIDs. Retry supplies the same map and activation fence.
    pub fn prepare_generator_admission(
        &self,
        encounter_ids: &BTreeMap<u32, EntityId>,
        options: crate::generator_preparation::GeneratorPreparationOptions,
    ) -> Result<PreparedGeneratorActivation, String> {
        self.prepare_generator_admission_with_loadouts(encounter_ids, options, &BTreeMap::new())
    }
    pub fn prepare_generator_admission_with_loadouts(
        &self,
        encounter_ids: &BTreeMap<u32, EntityId>,
        options: crate::generator_preparation::GeneratorPreparationOptions,
        loadouts: &BTreeMap<EntityId, bace_simulation::PreparedNpcLoadout>,
    ) -> Result<PreparedGeneratorActivation, String> {
        let revision = self.fence.content_generation;
        let epoch = self.fence.activation_epoch;
        if revision == 0
            || epoch == 0
            || self.content.landblock != self.fence.landblock
            || self.content.content_generation != revision
        {
            return Err("region admission fence mismatch".into());
        }
        let mut admission = PreparedGeneratorRegion {
            landblock: self.fence.landblock,
            epoch,
            revision,
            geometry: self.geometry.clone(),
            roots: vec![],
            containers: vec![],
            definitions: vec![],
            templates: vec![],
            creatures: vec![],
        };
        let mut held = Vec::new();
        for (&id, template) in &self.content.templates {
            if !template.properties.generators.is_empty() || template.weenie_type == 12 {
                let definition = crate::generator_preparation::prepare_generator(
                    template,
                    identity(EntityId(id), 1, revision),
                    GeneratorLocation {
                        cell: 0x00010001,
                        origin: [0.0; 3],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                    },
                    &[],
                    options,
                )?;
                admission.templates.push((id, definition));
            }
        }
        for (&id, creature) in &self.creatures {
            match creature {
                Ok(c) => admission.creatures.push((id, c.clone())),
                Err(e) => held.push((id, e.clone())),
            }
        }
        for instance in self
            .content
            .instances
            .iter()
            .filter(|i| !i.source.is_link_child)
        {
            let source = &instance.source;
            let location = GeneratorLocation {
                cell: source.obj_cell_id,
                origin: [source.origin_x, source.origin_y, source.origin_z],
                rotation: [
                    source.angles_x,
                    source.angles_y,
                    source.angles_z,
                    source.angles_w,
                ],
            };
            let links =
                crate::generator_preparation::region_generator_links(&self.content, source.guid)?;
            let owner = self
                .npc_recovery
                .get(&EntityId(source.guid))
                .map_or(self, |pin| pin.prepared.as_ref());
            if let Err(error) = owner.prepare_root(
                &mut admission,
                &instance.template,
                EntityId(source.guid),
                location,
                &links,
                options,
                loadouts.get(&EntityId(source.guid)),
            ) {
                held.push((source.guid, error));
            }
        }
        let mut ids = BTreeSet::new();
        for encounter in &self.content.encounters {
            let source = &encounter.source;
            let Some(&entity) = encounter_ids.get(&source.id) else {
                held.push((source.id, "encounter GUID not allocated".into()));
                continue;
            };
            if !(0x80000000..=0xfffffffe).contains(&entity.0) || !ids.insert(entity) {
                return Err("invalid or duplicate encounter GUID".into());
            }
            let position = crate::region_geometry::encounter_position(
                &self.geometry,
                self.fence.landblock,
                source.cell_x,
                source.cell_y,
            );
            match position {
                Ok((cell, p)) => {
                    if let Err(error) = self.prepare_root(
                        &mut admission,
                        &encounter.template,
                        entity,
                        GeneratorLocation {
                            cell,
                            origin: [p.x, p.y, p.z],
                            rotation: [0.0, 0.0, 0.0, 1.0],
                        },
                        &[],
                        options,
                        loadouts.get(&entity),
                    ) {
                        held.push((source.id, error));
                    }
                }
                Err(error) => held.push((source.id, error)),
            }
        }
        Ok(PreparedGeneratorActivation { admission, held })
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "cold source root joins authored placement, regional links and optional reserved loadout"
    )]
    fn prepare_root(
        &self,
        batch: &mut PreparedGeneratorRegion,
        template: &bace_content::WeenieV1,
        entity: EntityId,
        location: GeneratorLocation,
        links: &[bace_gameplay_api::GeneratorLink],
        options: crate::generator_preparation::GeneratorPreparationOptions,
        loadout: Option<&bace_simulation::PreparedNpcLoadout>,
    ) -> Result<(), String> {
        let creature = if crate::generator_preparation::is_creature_template(template.weenie_type) {
            Some(
                self.creatures
                    .get(&template.weenie_id)
                    .ok_or("static creature template absent")?
                    .as_ref()
                    .map_err(Clone::clone)?
                    .clone(),
            )
        } else {
            None
        };
        if creature.as_ref().is_some_and(|c| c.requires_equipment) && loadout.is_none() {
            return Err("creature equipment GUIDs/loadout not prepared".into());
        }
        let physical = self
            .physical
            .get(&template.weenie_id)
            .ok_or("root physical template not prepared")?
            .as_ref()
            .map_err(Clone::clone)?;
        let q = location.rotation;
        if q[0].abs() > 0.0002
            || q[1].abs() > 0.0002
            || !(0.999..=1.001).contains(&q.iter().map(|v| v * v).sum::<f32>())
        {
            return Err("unsupported root collision orientation".into());
        }
        let mut location = location;
        let visible: Vec<_> = self
            .visibility
            .iter()
            .find(|row| row.cell.0 == location.cell)
            .map(|row| row.visible_cells.iter().map(|cell| cell.0).collect())
            .unwrap_or_default();
        location.cell = self
            .geometry
            .placement_cell(
                location.cell,
                bace_geometry::Vec3::new(
                    location.origin[0],
                    location.origin[1],
                    location.origin[2],
                ),
                &physical.shape,
                &visible,
            )
            .map_err(|error| format!("root {:#x} cell {:#x}: {error}", entity.0, location.cell))?;
        self.geometry
            .validate_placement(
                location.cell,
                bace_geometry::Vec3::new(
                    location.origin[0],
                    location.origin[1],
                    location.origin[2],
                ),
                &physical.shape,
                &[],
                entity.0,
            )
            .map_err(|e| e.to_string())?;
        let definition = if template.properties.generators.is_empty() && template.weenie_type != 12
        {
            None
        } else {
            Some(crate::generator_preparation::prepare_generator(
                template,
                identity(entity, batch.epoch, batch.revision),
                location,
                links,
                options,
            )?)
        };
        let int = |id| {
            template
                .properties
                .ints
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.value)
        };
        let container = if crate::generator_preparation::is_container_template(template.weenie_type)
            && loadout.is_none()
        {
            let capacity = int(6).unwrap_or(0);
            Some(bace_inventory::InventoryContainer {
                id: entity,
                revision: 1,
                root_owner: None,
                slots: u32::try_from(capacity).map_err(|_| "negative root capacity")?,
                pack_slots: u32::try_from(int(7).unwrap_or(0))
                    .map_err(|_| "negative pack capacity")?,
                burden_limit: u64::try_from(int(96).unwrap_or(i32::MAX))
                    .map_err(|_| "negative burden limit")?,
                accessible: true,
                open: false,
                generation: batch.epoch,
            })
        } else {
            None
        };
        batch.roots.push(PreparedGeneratorRoot {
            script: None,
            entity,
            location,
            shape: physical.shape.clone(),
            creature,
            loadout: loadout.cloned(),
        });
        if let Some(container) = container {
            batch.containers.push(container);
        }
        if let Some(definition) = definition {
            batch.definitions.push(definition);
        }
        Ok(())
    }
}
pub(crate) fn identity(entity: EntityId, epoch: u64, revision: u64) -> GeneratorIdentity {
    let mut h = Sha256::new();
    h.update(b"BetterACE generator activation v1");
    h.update(entity.0.to_le_bytes());
    h.update(epoch.to_le_bytes());
    h.update(revision.to_le_bytes());
    let bytes = h.finalize();
    let mut random_identity = [0; 16];
    random_identity.copy_from_slice(&bytes[..16]);
    GeneratorIdentity {
        entity,
        incarnation: epoch,
        content_revision: revision,
        random_identity,
    }
}
