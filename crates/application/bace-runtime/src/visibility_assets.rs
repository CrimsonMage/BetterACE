//! Cold source descriptions. Dynamic pose, velocity and action fields are filled
//! only from a later accepted ObjectView. No saved pose is rendered as authority.
mod projectile;
use crate::player_entry::*;
use bace_content::WeenieV1;
use bace_types::EntityId;
use bace_wire::{ObjectDescription, PhysicsMovement, PhysicsParent};
pub use projectile::PreparedProjectileVisibilityTemplate;
use std::sync::Arc;
#[derive(Clone, Debug)]
pub struct PreparedVisibilityObject {
    pub incarnation: u64,
    pub revision: u64,
    pub description: Arc<ObjectDescription>,
    pub children: Vec<Arc<ObjectDescription>>,
}
impl PreparedVisibilityObject {
    pub fn register(
        &self,
        service: &mut crate::visibility_service::VisibilityService,
        accepted_tick: u64,
    ) -> Result<(), crate::visibility_service::VisibilityServiceError> {
        service.register_object(
            self.incarnation,
            self.revision,
            accepted_tick,
            self.description.clone(),
            self.children.clone(),
        )
    }
}
pub struct VisibilitySource<'a> {
    pub entity: EntityId,
    pub incarnation: u64,
    pub revision: u64,
    pub source: &'a WeenieV1,
    pub equipment: Vec<(EntityId, &'a WeenieV1, u32)>,
    pub missile_combat: bool,
}
pub fn prepare_visibility_object(
    source: VisibilitySource<'_>,
    assets: &EntryAppearanceAssets<'_>,
) -> Result<PreparedVisibilityObject, String> {
    if source.entity.0 == 0 || source.incarnation == 0 || source.revision == 0 {
        return Err("visibility identity/revision".into());
    }
    let creature = crate::generator_preparation::is_creature_template(source.source.weenie_type);
    let equipment: Vec<_> = source
        .equipment
        .iter()
        .map(|(id, item, location)| (id.0, *item, *location))
        .collect();
    let (children, attachments) =
        prepare_entry_attachments(source.entity.0, &equipment, source.missile_combat)?;
    let appearance = if creature {
        prepare_creature_model(
            source.source,
            &equipment
                .iter()
                .map(|(_, item, _)| *item)
                .collect::<Vec<_>>(),
            assets,
        )?
    } else {
        prepare_item_model(source.source, assets)?
    };
    let mut live = state(source.source, assets, creature)?;
    live.children = children;
    let description = Arc::new(prepare_entry_object(
        source.entity.0,
        source.source,
        appearance,
        live,
    )?);
    let mut child_descriptions = Vec::new();
    for attachment in attachments.into_iter().filter(|a| a.parent.is_some()) {
        let item = equipment
            .iter()
            .find(|(id, _, _)| *id == attachment.item)
            .ok_or("attachment source absent")?
            .1;
        let mut state = state(item, assets, false)?;
        state.parent = attachment.parent;
        state.movement = Some(PhysicsMovement::AnimationFrame(attachment.placement));
        child_descriptions.push(Arc::new(prepare_entry_object(
            attachment.item,
            item,
            prepare_item_model(item, assets)?,
            state,
        )?));
    }
    Ok(PreparedVisibilityObject {
        incarnation: source.incarnation,
        revision: source.revision,
        description,
        children: child_descriptions,
    })
}
fn state(
    source: &WeenieV1,
    assets: &EntryAppearanceAssets<'_>,
    creature: bool,
) -> Result<EntryObjectState, String> {
    let flags = match source
        .properties
        .data_ids
        .iter()
        .find(|p| p.id == 1)
        .map(|p| p.value)
        .filter(|id| *id != 0)
    {
        Some(setup) => {
            assets
                .setups
                .get(&setup)
                .ok_or("visible object setup absent")?
                .flags
        }
        None => 0,
    };
    let sequences =
        bace_replication::Sequences::new(10).map_err(|_| "visibility sequence capacity")?;
    Ok(EntryObjectState {
        is_player: false,
        is_creature: creature,
        physics_state: initial_physics_state(source, flags, false),
        position: None,
        movement: None,
        parent: None::<PhysicsParent>,
        children: vec![],
        velocity: [0.0; 3],
        acceleration: [0.0; 3],
        omega: [0.0; 3],
        sequences: bace_replication::physics_sequences(&sequences),
        admin_vision: false,
        change_no_draw: false,
        cloak_status: 0,
    })
}
