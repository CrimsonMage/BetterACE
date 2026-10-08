//! Pure source-property adaptation. Authored profile ordering and nullable
//! placement fields survive preparation; no pose becomes accepted here.
mod creatures;
use bace_content::WeenieTemplate;
use bace_gameplay_api::generators::*;
pub use creatures::{CreatureAdmissionAssets, CreatureAdmissionPolicy, prepare_creature};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct GeneratorPreparationOptions {
    pub use_rotation_offset: bool,
    pub vendor_shop_uses_generator: bool,
    pub drop_plain_wield: bool,
}
impl Default for GeneratorPreparationOptions {
    fn default() -> Self {
        Self {
            use_rotation_offset: true,
            vendor_shop_uses_generator: false,
            drop_plain_wield: false,
        }
    }
}
pub fn prepare_generator(
    template: &WeenieTemplate,
    identity: GeneratorIdentity,
    location: GeneratorLocation,
    links: &[GeneratorLink],
    options: GeneratorPreparationOptions,
) -> Result<Arc<GeneratorDefinition>, String> {
    let p = &template.properties;
    let int = |id| p.ints.iter().find(|p| p.id == id).map_or(0, |p| p.value);
    let float = |id| {
        p.floats
            .iter()
            .find(|p| p.id == id)
            .map_or(0.0, |p| p.value)
    };
    let boolean = |id| p.bools.iter().find(|p| p.id == id).is_some_and(|p| p.value);
    let destruction = |id| match int(id) {
        0 | 1 => Ok(GeneratorDestruction::Nothing),
        2 => Ok(GeneratorDestruction::Destroy),
        3 => Ok(GeneratorDestruction::Kill),
        _ => Err("invalid generator destruction"),
    };
    let profiles = p
        .generators
        .iter()
        .enumerate()
        .map(|(index, g)| GeneratorProfile {
            id: index as u32,
            probability: g.probability,
            weenie_class_id: g.weenie_class_id,
            delay: g.delay,
            init_create: g.init_create,
            max_create: g.max_create,
            when_create: g.when_create,
            where_create: g.where_create,
            stack_size: g.stack_size,
            palette_id: g.palette_id,
            shade: g.shade,
            position: GeneratorPositionSpec {
                cell: g.obj_cell_id,
                origin: [g.origin_x, g.origin_y, g.origin_z],
                rotation: [g.angles_x, g.angles_y, g.angles_z, g.angles_w],
            },
        })
        .collect();
    let mut definition = GeneratorDefinition {
        identity,
        profiles,
        location,
        kind: match template.weenie_type {
            10 | 15 | 61 | 69 | 71 => GeneratorKind::Creature,
            12 => GeneratorKind::Vendor,
            20 => GeneratorKind::Chest,
            14 | 21 | 55 | 56 | 57 => GeneratorKind::Container,
            _ => GeneratorKind::Object,
        },
        initial_count: int(82),
        maximum_count: int(81),
        regeneration_interval: float(41),
        initial_delay: float(121),
        regeneration_timestamp: float(113),
        time_type: match int(142) {
            0 => GeneratorTimeType::Undefined,
            1 => GeneratorTimeType::RealTime,
            2 => GeneratorTimeType::Defined,
            3 => GeneratorTimeType::Event,
            4 => GeneratorTimeType::Night,
            5 => GeneratorTimeType::Day,
            _ => return Err("invalid generator time type".into()),
        },
        event: p
            .strings
            .iter()
            .find(|p| p.id == 34)
            .map(|p| p.value.clone())
            .filter(|s| !s.is_empty()),
        start_time: int(143),
        end_time: int(144),
        disabled: boolean(59),
        automatic_destruction: boolean(74),
        parent: p
            .instance_ids
            .iter()
            .find(|p| p.id == 6)
            .map(|p| bace_types::EntityId(p.value))
            .filter(|v| v.0 != 0),
        destruction: destruction(103)?,
        end_destruction: destruction(145)?,
        rotation_type: match int(100) {
            0 => GeneratorRotationType::Undefined,
            1 => GeneratorRotationType::Relative,
            2 => GeneratorRotationType::Absolute,
            _ => return Err("invalid generator rotation type".into()),
        },
        use_rotation_offset: options.use_rotation_offset,
        radius: float(43) as f32,
        vendor_shop_uses_generator: options.vendor_shop_uses_generator,
    };
    bace_spawning::append_generator_links(&mut definition, links)
        .map_err(|e| format!("invalid generator links: {e:?}"))?;
    Ok(Arc::new(definition))
}
/// Official factory resolves links only inside this region's source-object list.
/// Link children are not independent roots. Missing authored targets stay absent.
pub fn region_generator_links(
    region: &crate::world_content::PreparedRegion,
    parent: u32,
) -> Result<Vec<GeneratorLink>, String> {
    let instance = region
        .instances
        .iter()
        .find(|i| i.source.guid == parent)
        .ok_or("missing link parent")?;
    Ok(instance
        .links
        .iter()
        .filter_map(|link| {
            let child = region
                .instances
                .iter()
                .find(|i| i.source.guid == link.child_guid)?;
            let s = &child.source;
            Some(GeneratorLink {
                profile_id: s.guid,
                weenie_class_id: s.weenie_class_id,
                location: GeneratorLocation {
                    cell: s.obj_cell_id,
                    origin: [s.origin_x, s.origin_y, s.origin_z],
                    rotation: [s.angles_x, s.angles_y, s.angles_z, s.angles_w],
                },
            })
        })
        .collect())
}

/// WorldObjectFactory.CreateWorldObject at the official pin. AI (16) falls
/// through to GenericObject; Vendor is a Creature despite its generator kind.
pub(crate) fn is_creature_template(weenie_type: u32) -> bool {
    matches!(weenie_type, 10 | 12 | 15 | 61 | 69 | 71)
}
#[cfg(test)]
mod constructor_tests;

/// Constructor inheritance, independently covered by the compiled ACE factory.
pub(crate) fn is_container_template(weenie_type: u32) -> bool {
    is_creature_template(weenie_type) || matches!(weenie_type, 14 | 20 | 21 | 55 | 56 | 57)
}
