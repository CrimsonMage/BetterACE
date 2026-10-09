//! Cold composition of verified geometry and source-prepared player capabilities.
//! This does not synthesize avatar physics, weapon maneuvers or account authority.
use crate::game_login::LoadedPlayer;
use bace_simulation::{OwnedPlayerState, PlayerWorldSnapshot, PreparedPlayerAdmission};
use bace_types::EntityId;
use std::sync::Arc;
pub struct PlayerAdmissionCapabilities {
    pub chat_eligibility: bace_social::ChatEligibility,
    pub locomotion: Arc<bace_motion::AnimatedLocomotion>,
    pub locomotion_styles: Vec<Arc<bace_motion::AnimatedLocomotion>>,
    pub death_motions: Vec<bace_motion::PreparedDeathMotion>,
    pub properties: bace_entity::EntityProperties,
    pub combatant: bace_entity::Combatant,
    pub caster: bace_simulation::MagicCaster,
    pub magic_damage: bace_magic::MagicDamageProfile,
    pub physical: Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>,
    pub physical_motions: Vec<crate::physical_assets::PhysicalMotionRegistration>,
    pub physical_source: Arc<bace_combat::preparation::PreparedPhysicalRefreshSource>,
    pub skills: bace_simulation::PreparedCharacterSkillInputs,
    pub vital_inputs: bace_simulation::PreparedVitalInputs,
    pub items: Vec<bace_inventory::InventoryItem>,
    pub containers: Vec<bace_inventory::InventoryContainer>,
    pub presence: bace_social::SocialPresence,
    pub staff: bace_gameplay_api::staff::StaffRegistration,
    pub portal_access: bace_interactions::PortalAccess,
}
pub fn prepare_player_admission(
    loaded: &LoadedPlayer,
    mut state: OwnedPlayerState,
    region: &bace_physics::GeometryRegion,
    spawn: bace_physics::GeometrySpawn,
    capabilities: PlayerAdmissionCapabilities,
) -> Result<PreparedPlayerAdmission, String> {
    let actor = loaded.binding.actor;
    let saved = &loaded.player.player.entity;
    if saved.object_id != actor.0
        || loaded.player.player.account_id != loaded.binding.account.0
        || capabilities.staff.binding != loaded.binding
        || capabilities.items.len() != loaded.inventory.len()
        || loaded.inventory.len() > 1023
    {
        return Err("player admission identity/count mismatch".into());
    }
    let position = saved
        .state
        .properties
        .positions
        .iter()
        .find(|p| p.id == 1)
        .ok_or("saved player location missing")?;
    let p = &position.value;
    if spawn.cell != p.obj_cell_id
        || spawn.position != bace_geometry::Vec3::new(p.position_x, p.position_y, p.position_z)
    {
        return Err("player admission differs from saved authoritative location".into());
    }
    if [p.rotation_w, p.rotation_x, p.rotation_y, p.rotation_z]
        .iter()
        .any(|v| !v.is_finite())
        || p.rotation_x.abs() > 0.0001
        || p.rotation_y.abs() > 0.0001
    {
        return Err("unsupported player root orientation".into());
    }
    let heading = 2.0 * p.rotation_z.atan2(p.rotation_w);
    if (spawn.heading - heading).sin().abs() > 0.00001 || (spawn.heading - heading).cos() < 0.99999
    {
        return Err("player admission heading mismatch".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for item in &capabilities.items {
        if !ids.insert(item.id) {
            return Err("duplicate prepared player item".into());
        }
        let source = loaded
            .inventory
            .iter()
            .find(|i| i.entity.object_id == item.id.0)
            .ok_or("unowned prepared player item")?;
        let bace_storage_codec::ItemPlacementV2::Contained {
            container,
            slot,
            pack_slot,
            equipped,
        } = source.placement
        else {
            return Err("player item not durably contained".into());
        };
        if item.revision != source.entity.mutation_revision
            || item.template != source.entity.state.weenie_id
            || item.pack_slot != pack_slot
            || item.place
                != (bace_inventory::ItemPlace::Contained {
                    container: EntityId(container),
                    slot,
                    equipped,
                })
        {
            return Err("prepared player item ownership mismatch".into());
        }
        if !state.item_enchantments.iter().any(|(id, _)| *id == item.id) {
            if !source.enchantments.is_empty() {
                return Err("missing restored item enchantments".into());
            }
            state.item_enchantments.push((
                item.id,
                bace_magic::EnchantmentRegistry::new(4096)
                    .map_err(|e| format!("empty registry: {e:?}"))?,
            ));
        }
    }
    if state.recovery.is_none() {
        if loaded.player.combat_recovery.is_some() {
            return Err("missing restored cast recovery".into());
        }
        state.recovery = Some(bace_magic::CastRecovery::default());
    }
    let spawn_cell = spawn.cell;
    let body = bace_physics::Body::spawn_geometry(region, spawn)
        .map_err(|e| format!("player geometry admission: {e:?}"))?;
    // Geometry can settle a saved spawn before the simulation owns the body.
    // That accepted pose is a real player-save change even though the world's
    // first seen snapshot will already equal it. Carry a dirty character
    // revision into admission so entry freeze and the routine save agree.
    let accepted = body.accepted();
    let accepted_heading = accepted.heading_radians();
    let accepted_position = accepted.position();
    let accepted_half = accepted_heading * 0.5;
    let settled = bace_content::Position {
        obj_cell_id: spawn_cell,
        position_x: accepted_position.x,
        position_y: accepted_position.y,
        position_z: accepted_position.z,
        rotation_w: accepted_half.cos(),
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: accepted_half.sin(),
    };
    if *p != settled {
        state
            .character
            .progression
            .touch_revision()
            .map_err(|e| format!("settled player pose revision: {e:?}"))?;
    }
    let kinds = [
        bace_entity::EntityVital::Health,
        bace_entity::EntityVital::Stamina,
        bace_entity::EntityVital::Mana,
    ];
    let mut vitals = [None; 3];
    for (index, kind) in kinds.into_iter().enumerate() {
        let pool = capabilities
            .combatant
            .vital(kind)
            .ok_or("missing player vital capability")?;
        let source = saved
            .state
            .properties
            .secondary_attributes
            .iter()
            .find(|p| p.id == [1, 3, 5][index])
            .ok_or("saved player vital missing")?;
        if source.value.current_level != pool.current {
            return Err("prepared vital differs from saved player".into());
        }
        vitals[index] = Some(pool);
    }
    state.world = Some(PlayerWorldSnapshot {
        cell: bace_types::CellId(p.obj_cell_id),
        position: body.accepted().position(),
        heading: body.accepted().heading_radians(),
        vitals,
    });
    let item_spell_targets = loaded
        .inventory
        .iter()
        .map(|saved| {
            let properties = &saved.entity.state.properties;
            let integer = |key| {
                properties
                    .ints
                    .iter()
                    .find(|p| p.id == key)
                    .map_or(0, |p| p.value)
            };
            Ok(bace_simulation::PreparedItemSpellTarget {
                item: EntityId(saved.entity.object_id),
                item_type: integer(1) as u32,
                resist_magic: u32::try_from(integer(36))
                    .map_err(|_| "negative item magic resistance")?,
                non_projectile_immune: properties.bools.iter().any(|p| p.id == 103 && p.value),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PreparedPlayerAdmission {
        binding: loaded.binding,
        actor: bace_entity::Actor {
            id: actor,
            cell: bace_types::CellId(p.obj_cell_id),
            body,
        },
        state,
        locomotion: capabilities.locomotion,
        locomotion_styles: capabilities.locomotion_styles,
        death_motions: capabilities.death_motions,
        server_magic: bace_simulation::PreparedMagicAssetBatch {
            definitions: vec![],
            projectile_shapes: vec![],
        },
        properties: capabilities.properties,
        combatant: capabilities.combatant,
        caster: capabilities.caster,
        magic_damage: capabilities.magic_damage,
        physical: capabilities.physical,
        physical_motions: capabilities.physical_motions,
        physical_source: capabilities.physical_source,
        skills: capabilities.skills,
        vital_inputs: capabilities.vital_inputs,
        items: capabilities.items,
        item_spell_targets,
        containers: capabilities.containers,
        presence: capabilities.presence,
        chat_eligibility: capabilities.chat_eligibility,
        staff: capabilities.staff,
        portal_access: capabilities.portal_access,
    })
}
