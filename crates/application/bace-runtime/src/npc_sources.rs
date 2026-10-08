//! Cold immutable native source definitions delivered only after real regional
//! actor admission. Dynamic generators use the same template hash producer.
use crate::npc_recovery::{PreparedNpcSource, prepare_npc_source};
use bace_storage_codec::PackGeneration;
use bace_types::EntityId;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Clone)]
pub struct PreparedNpcRegistration {
    pub admitted: bool,
    pub object_registry: Option<bace_simulation::PreparedNpcRegistryRestore>,
    pub baseline: Option<bace_persistence::StoredAggregate>,
    pub actor: EntityId,
    pub landblock: u16,
    pub epoch: u64,
    pub content_hash: [u8; 32],
    pub generation: Arc<PackGeneration>,
    pub source: Arc<PreparedNpcSource>,
    pub use_radius: f32,
}
pub fn prepare_registration(
    actor: EntityId,
    landblock: u16,
    epoch: u64,
    content_hash: [u8; 32],
    generation: Arc<PackGeneration>,
    template: u32,
) -> Result<PreparedNpcRegistration, String> {
    if actor.0 == 0 || epoch == 0 || content_hash == [0; 32] {
        return Err("NPC registration identity".into());
    }
    let source = Arc::new(prepare_npc_source(&generation, template)?);
    // Pinned ACE WorldObject_Use.IsWithinUseRadiusOf retains signed Float54;
    // only absence selects0.6. Source rows may be non-interactive while their
    // other emotes still require admission.
    let use_radius = match source
        .properties
        .get(bace_entity::PropertyFamily::Float, 54)
    {
        Some(bace_entity::PropertyValue::Float(radius)) => *radius as f32,
        _ => 0.6,
    };
    if !use_radius.is_finite() {
        return Err(format!("NPC source {template} use radius is not finite"));
    }
    Ok(PreparedNpcRegistration {
        admitted: false,
        object_registry: None,
        baseline: None,
        actor,
        landblock,
        epoch,
        content_hash,
        generation,
        source,
        use_radius,
    })
}
pub(crate) fn prepare_region(
    prepared: &crate::region_activation::PreparedRegionActivation,
    encounters: &BTreeMap<u32, EntityId>,
    admitted: &BTreeSet<EntityId>,
    content_hash: [u8; 32],
) -> Result<Vec<PreparedNpcRegistration>, String> {
    let mut cache: BTreeMap<u32, Arc<PreparedNpcSource>> = BTreeMap::new();
    let mut result = Vec::new();
    let mut bytes = 0usize;
    for (actor, template) in prepared
        .content
        .instances
        .iter()
        .filter(|i| !i.source.is_link_child)
        .map(|i| (EntityId(i.source.guid), &i.template))
        .chain(
            prepared
                .content
                .encounters
                .iter()
                .filter_map(|e| encounters.get(&e.source.id).map(|id| (*id, &e.template))),
        )
    {
        if admitted.contains(&actor)
            && let Some(pinned) = prepared.npc_recovery.get(&actor)
        {
            bytes = bytes
                .checked_add(pinned.registration.source.retained_bytes)
                .ok_or("NPC source registration byte overflow")?;
            if bytes > 64 * 1024 * 1024 {
                return Err("NPC source registration byte capacity".into());
            }
            result.push(pinned.registration.clone());
            continue;
        }
        if !admitted.contains(&actor) || !requires_source(template) {
            continue;
        }
        if result.len() == 4096 {
            return Err("NPC region registration capacity".into());
        }
        let mut registration = prepare_registration(
            actor,
            prepared.fence.landblock,
            prepared.fence.activation_epoch,
            content_hash,
            prepared.generation.clone(),
            template.weenie_id,
        )?;
        registration.admitted = true;
        if let Some(source) = cache.get(&template.weenie_id) {
            registration.source = source.clone();
        } else {
            bytes += registration.source.retained_bytes;
            if bytes > 64 * 1024 * 1024 {
                return Err("NPC source byte budget".into());
            }
            cache.insert(template.weenie_id, registration.source.clone());
        }
        result.push(registration);
    }
    Ok(result)
}

impl PreparedNpcRegistration {
    pub fn script_identity(&self) -> bace_simulation::NpcScriptIdentity {
        bace_simulation::NpcScriptIdentity {
            template: self.source.template,
            program_hash: self.source.program_hash,
            content_generation: self.content_hash,
        }
    }
    pub fn script_source(&self) -> bace_simulation::PreparedNpcScriptSource {
        bace_simulation::PreparedNpcScriptSource {
            identity: self.script_identity(),
            program: self.source.program.clone(),
            properties: self.source.properties.clone(),
            use_radius: self.use_radius,
        }
    }
}

pub fn requires_source(source: &bace_content::WeenieV1) -> bool {
    !source.properties.emotes.is_empty()
        || source
            .properties
            .bools
            .iter()
            .any(|p| p.id == 79 && p.value)
}
/// The caller supplies the exact cold actor row and immutable selected generation.
pub fn prepare_generated_registration(
    actor: EntityId,
    epoch: u64,
    landblock: u16,
    generation: Arc<PackGeneration>,
    content_hash: [u8; 32],
    actual: &bace_content::WeenieV1,
) -> Result<Option<PreparedNpcRegistration>, String> {
    if !requires_source(actual) {
        return Ok(None);
    }
    let mut registration = prepare_registration(
        actor,
        landblock,
        epoch,
        content_hash,
        generation,
        actual.weenie_id,
    )?;
    if actual.properties.emotes != registration.source.authored.properties.emotes {
        return Err("NPC generated program differs from accepted immutable definition".into());
    }
    let source = Arc::make_mut(&mut registration.source);
    source.authored = Arc::new(actual.clone());
    source.properties = crate::npc_recovery::source_properties(actual)?;
    registration.admitted = true;
    Ok(Some(registration))
}
