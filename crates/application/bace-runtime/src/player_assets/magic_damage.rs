//! Join exact durable equipment identities with domain-owned magic properties.
use super::*;
pub fn prepare_player_magic_damage(
    loaded: &crate::game_login::LoadedPlayer,
    state: &bace_simulation::OwnedPlayerState,
    stats: &PlayerStatProjection,
) -> Result<bace_magic::MagicDamageProfile, String> {
    if loaded.inventory.len() > 1023 {
        return Err("player magic equipment capacity".into());
    }
    let mut equipment = Vec::new();
    for item in &loaded.inventory {
        if let bace_storage_codec::ItemPlacementV2::Contained {
            container,
            equipped,
            ..
        } = item.placement
            && equipped != 0
            && container == loaded.binding.actor.0
        {
            if equipment.len() >= 64 {
                return Err("invalid equipped magic owner".into());
            }
            equipment.push(bace_magic::MagicDamageEquipment {
                entity: item.entity.object_id,
                revision: item.entity.mutation_revision,
                location: equipped,
                level: state
                    .item_experience
                    .iter()
                    .find(|p| p.item.0 == item.entity.object_id)
                    .and_then(|p| p.experience)
                    .map(|xp| xp.level().map_err(|e| format!("magic item level: {e:?}")))
                    .transpose()?,
                weenie: &item.entity.state,
            });
        }
    }
    let shield = stats
        .skills
        .inputs
        .iter()
        .find(|(id, _)| *id == 48)
        .map(|(_, input)| {
            state
                .character
                .progression
                .skill_values(48, *input)
                .map(|value| value.base)
                .map_err(|e| format!("shield base skill: {e:?}"))
        })
        .transpose()?
        .unwrap_or(0);
    bace_magic::prepare_magic_damage_profile(bace_magic::MagicDamagePreparation {
        player: true,
        weenie: &loaded.player.player.entity.state,
        equipment: &equipment,
        skills: &stats.physical_skills,
        base_attributes: stats.base_attributes,
        base_shield_skill: shield,
    })
    .map_err(|e| format!("player magic damage preparation: {e:?}"))
}
