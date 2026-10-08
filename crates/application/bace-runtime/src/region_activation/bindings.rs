//! Cold binding-object metadata from accepted authored region roots.
use super::PreparedRegionActivation;
use bace_interactions::BindingKind;
use bace_simulation::{PreparedBindingObject, PreparedGeneratorRegion};
use bace_types::EntityId;
use std::{collections::BTreeSet, sync::Arc};

pub(crate) fn prepare_binding_objects(
    prepared: &PreparedRegionActivation,
    region: &PreparedGeneratorRegion,
) -> Result<Vec<PreparedBindingObject>, String> {
    let roots: BTreeSet<_> = region.roots.iter().map(|root| root.entity).collect();
    let mut seen = BTreeSet::new();
    let mut objects = Vec::new();
    for instance in prepared
        .content
        .instances
        .iter()
        .filter(|instance| !instance.source.is_link_child)
    {
        let kind = match instance.template.weenie_type {
            25 => BindingKind::Lifestone,
            65 => BindingKind::Allegiance,
            _ => continue,
        };
        let entity = EntityId(instance.source.guid);
        if entity.0 == 0 || !roots.contains(&entity) || !seen.insert(entity) {
            return Err("binding object has no unique admitted region root".into());
        }
        let use_radius = instance
            .template
            .properties
            .floats
            .iter()
            .find(|property| property.id == 54)
            .map_or(0.6_f32, |property| property.value as f32);
        if !use_radius.is_finite() || !(0.0..=100.0).contains(&use_radius) {
            return Err("binding object authored UseRadius is invalid".into());
        }
        let use_message = instance
            .template
            .properties
            .strings
            .iter()
            .find(|property| property.id == 18)
            .map_or("", |property| property.value.as_str());
        if use_message.len() > 4096 || objects.len() >= 4096 {
            return Err("binding object source message/capacity".into());
        }
        objects.push(PreparedBindingObject {
            entity,
            kind,
            use_radius,
            use_message: Arc::from(use_message),
        });
    }
    Ok(objects)
}
