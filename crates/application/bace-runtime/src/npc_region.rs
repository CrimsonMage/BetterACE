//! Actor-specific historical source closure. Equal WCIDs in different accepted
//! generations never overwrite one another's physical or spell definitions.
mod registries;
pub(crate) use registries::{restore_loadout, restore_object_registry};
use std::sync::Arc;
pub(crate) struct NpcRegionRequest {
    pub baseline: Option<bace_persistence::StoredAggregate>,
    pub source: crate::npc_recovery::NpcRecoveryRequest,
    pub inventory: Vec<bace_persistence::LocatedSnapshot>,
}
pub struct PreparedNpcRegionSource {
    pub inventory: Option<bace_storage_codec::npc_workflow_v3::NpcSourceInventoryV3>,
    pub items: Arc<Vec<bace_persistence::LocatedSnapshot>>,
    pub registration: crate::npc_sources::PreparedNpcRegistration,
    pub location: bace_simulation::NpcSourceLocation,
    pub prepared: Arc<crate::region_activation::PreparedRegionActivation>,
}
/// This adapter row comes from accepted World state in the source journal. It is
/// never treated as authored placement or client-supplied movement.
pub(crate) fn accepted_instance(
    actor: u32,
    template: &Arc<bace_content::WeenieV1>,
    location: bace_simulation::NpcSourceLocation,
) -> crate::world_content::PreparedInstance {
    let (sine, cosine) = (location.heading * 0.5).sin_cos();
    crate::world_content::PreparedInstance {
        source: bace_content::LandblockInstanceRowV1 {
            guid: actor,
            landblock: (location.cell.0 >> 16) as i32,
            weenie_class_id: template.weenie_id,
            obj_cell_id: location.cell.0,
            origin_x: location.position.x,
            origin_y: location.position.y,
            origin_z: location.position.z,
            angles_w: cosine,
            angles_x: 0.0,
            angles_y: 0.0,
            angles_z: sine,
            is_link_child: false,
            last_modified: String::new(),
        },
        template: template.clone(),
        links: vec![],
    }
}
pub(crate) fn install(
    prepared: &mut crate::region_activation::PreparedRegionActivation,
    pinned: PreparedNpcRegionSource,
) -> Result<(), String> {
    let actor = pinned.registration.actor;
    if pinned.location.cell.0 >> 16 != u32::from(prepared.fence.landblock)
        || prepared.npc_recovery.contains_key(&actor)
    {
        return Err("NPC pinned region identity/conflict".into());
    }
    let source = pinned
        .prepared
        .content
        .instances
        .first()
        .ok_or("NPC pinned source root missing")?;
    let instance = accepted_instance(actor.0, &source.template, pinned.location);
    if let Some(existing) = prepared
        .content
        .instances
        .iter_mut()
        .find(|i| i.source.guid == actor.0)
    {
        *existing = instance;
    } else {
        if prepared.content.instances.len() + prepared.content.encounters.len() >= 4096 {
            return Err("NPC restored source region capacity".into());
        }
        prepared.content.instances.push(instance);
    }
    prepared.npc_recovery.insert(actor, Arc::new(pinned));
    Ok(())
}
pub(crate) async fn request(
    store: &bace_db_postgres::PgStore,
    head: bace_db_postgres::StoredNpcSourceHead,
) -> Result<NpcRegionRequest, String> {
    let checkpoint = bace_storage_codec::NpcWorkflowSaveV3::decode_or_migrate(&head.checkpoint)
        .map_err(|e| e.to_string())?;
    if checkpoint.archive.is_some()
        || checkpoint.source_template == 0
        || checkpoint.location.is_none()
    {
        return Err("NPC live source locator is unavailable".into());
    }
    let accepted = store
        .generation_by_hash(checkpoint.content_generation)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("NPC pinned generation unavailable")?;
    let manifest =
        bace_storage_codec::PackManifest::decode(&accepted.manifest_bytes, Default::default())
            .map_err(|e| e.to_string())?;
    if manifest
        .content_hash(Default::default())
        .map_err(|e| e.to_string())?
        != checkpoint.content_generation
        || manifest.base.generation != accepted.base_hash
    {
        return Err("NPC pinned manifest identity".into());
    }
    let inventory = if checkpoint.inventory.is_some() {
        store
            .load_npc_source_inventory(head.source, head.source_version)
            .await
            .map_err(|e| e.to_string())?
    } else {
        vec![]
    };
    Ok(NpcRegionRequest {
        baseline: store.load(head.source).await.map_err(|e| e.to_string())?,
        source: crate::npc_recovery::NpcRecoveryRequest {
            head,
            manifest,
            manifest_hash: checkpoint.content_generation,
        },
        inventory,
    })
}
