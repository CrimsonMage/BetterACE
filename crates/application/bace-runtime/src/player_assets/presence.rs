//! Login presence and sparse property projection from the accepted player save.
use super::*;
use bace_entity::{EntityProperties, PropertyFamily as F, PropertyValue as V};
pub(super) fn prepare_presence(
    loaded: &crate::game_login::LoadedPlayer,
    state: &bace_simulation::OwnedPlayerState,
    access: u8,
) -> Result<bace_social::SocialPresence, String> {
    let props = &loaded.player.player.entity.state.properties;
    let ui = &state
        .character
        .ui
        .as_ref()
        .ok_or("missing player UI")?
        .state;
    let boolean = |id| {
        props
            .bools
            .iter()
            .find(|p| p.id == id)
            .is_some_and(|p| p.value)
    };
    let integer = |id| {
        props
            .ints
            .iter()
            .find(|p| p.id == id)
            .map_or(0, |p| p.value)
    };
    let o1 = |mask| ui.options1 & mask != 0;
    let o2 = |mask| ui.options2 & mask != 0;
    Ok(bace_social::SocialPresence {
        identity: bace_gameplay_api::social::SocialIdentity {
            character: loaded.binding.actor,
            account: loaded.binding.account,
            name: loaded.player.player.name.clone(),
        },
        access,
        online: true,
        appear_offline: o2(0x1000),
        afk: false,
        gagged: boolean(111),
        olthoi: integer(188) == 13,
        no_olthoi_talk: boolean(129),
        ignore_fellowship_requests: o1(8),
        auto_accept_fellowship: o1(0x20000000),
        share_fellowship_loot: o1(0x100000),
        society: match integer(281) {
            0 => 0,
            1 => 7,
            2 => 8,
            4 => 9,
            _ => return Err("invalid society faction".into()),
        },
        listen_allegiance: o1(0x40000000),
        listen_general: o2(0x100),
        listen_trade: o2(0x200),
        listen_lfg: o2(0x400),
        listen_roleplay: o2(0x800),
        listen_society: o2(0x80000),
    })
}
pub(super) fn prepare_properties(
    source: &bace_content::WeenieV1,
    stats: &PlayerStatProjection,
) -> Result<EntityProperties, String> {
    let mut result =
        EntityProperties::new(4096).map_err(|e| format!("player property capacity: {e:?}"))?;
    let mut insert = |family, key, value| {
        let change = result
            .propose(family, key, Some(value))
            .map_err(|e| format!("player property projection: {e:?}"))?;
        result
            .adopt(change)
            .map_err(|e| format!("player property adoption: {e:?}"))
    };
    for p in &source.properties.ints {
        insert(F::Int, p.id, V::Int(p.value))?;
    }
    for p in &source.properties.int64s {
        insert(F::Int64, p.id, V::Int64(p.value))?;
    }
    for p in &source.properties.bools {
        insert(F::Bool, p.id, V::Bool(p.value))?;
    }
    for p in &source.properties.floats {
        insert(F::Float, p.id, V::Float(p.value))?;
    }
    for p in &source.properties.strings {
        insert(F::String, p.id, V::String(p.value.clone()))?;
    }
    for (index, value) in stats.base_attributes.iter().enumerate() {
        insert(F::RawAttribute, index as u32 + 1, V::Unsigned(*value))?;
        insert(
            F::Attribute,
            index as u32 + 1,
            V::Unsigned(stats.current_attributes[index]),
        )?;
    }
    for (id, skill) in &stats.physical_skills {
        insert(F::Skill, *id, V::Unsigned(skill.current))?;
        insert(F::SkillAdvancement, *id, V::Unsigned(skill.advancement))?;
    }
    for (id, vital) in [1, 3, 5].into_iter().zip(stats.vitals) {
        insert(F::Vital, id, V::Unsigned(vital.maximum))?;
    }
    Ok(result)
}
