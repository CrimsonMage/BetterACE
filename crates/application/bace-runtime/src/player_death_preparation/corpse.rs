//! Official ACE Creature_Death.CreateCorpse and Corpse.SetEphemeralValues.
use crate::game_inventory::set;
use bace_content::{AnimationPart, Palette, Position, TextureMap, WeenieV1};
use bace_types::EntityId;

pub struct CorpseAppearanceInput<'a> {
    pub template: &'a WeenieV1,
    pub player: &'a WeenieV1,
    pub actor: EntityId,
    pub position: Position,
    pub model: &'a bace_wire::ObjectModel,
    /// Already resolved highest-damage killer, with combat-pet owner mapping.
    pub killer: Option<(EntityId, &'a str)>,
}
/// Copy appearance at death, retaining the corpse template's sound and its
/// intrinsic qualities. A source avatar's translucency is intentionally absent.
pub fn prepare_corpse_source(input: CorpseAppearanceInput<'_>) -> Result<WeenieV1, String> {
    let CorpseAppearanceInput {
        template,
        player,
        actor,
        position,
        model,
        killer,
    } = input;
    if template.weenie_type != 14
        || actor.0 == 0
        || model.parts.len() > 255
        || model.palettes.len() > 255
        || model.textures.len() > 255
    {
        return Err("invalid corpse source identity/model".into());
    }
    let name = player
        .properties
        .strings
        .iter()
        .find(|p| p.id == 1)
        .ok_or("death player name missing")?;
    if name.value.len() > 100 || killer.is_some_and(|(_, name)| name.len() > 100) {
        return Err("death source name bounds".into());
    }
    let mut corpse = template.clone();
    let treasure = player
        .properties
        .bools
        .iter()
        .any(|p| p.id == 120 && p.value);
    if player
        .properties
        .bools
        .iter()
        .any(|p| p.id == 29 && p.value)
    {
        return Err("NoCorpse player death needs source world-drop transaction".into());
    }
    if treasure {
        set(&mut corpse.properties.data_ids, 1, 0x02000ec4);
        set(&mut corpse.properties.data_ids, 2, 0x0900019b);
        set(&mut corpse.properties.data_ids, 3, 0x200000c2);
        set(&mut corpse.properties.floats, 39, f64::from(0.4f32));
    } else {
        for id in [1, 2, 6, 7, 22] {
            corpse.properties.data_ids.retain(|p| p.id != id);
            if let Some(value) = player.properties.data_ids.iter().find(|p| p.id == id) {
                set(&mut corpse.properties.data_ids, id, value.value);
            } else if matches!(id, 1 | 2 | 22) {
                return Err("death avatar model data missing".into());
            }
        }
        for id in [39, 12] {
            if let Some(value) = player.properties.floats.iter().find(|p| p.id == id) {
                set(&mut corpse.properties.floats, id, value.value);
            }
        }
        if let Some(value) = player.properties.ints.iter().find(|p| p.id == 3) {
            set(&mut corpse.properties.ints, 3, value.value);
        }
        corpse.properties.animation_parts = model
            .parts
            .iter()
            .map(|p| AnimationPart {
                index: p.part_index,
                animation_id: p.animation_id,
            })
            .collect();
        corpse.properties.palettes = model
            .palettes
            .iter()
            .map(|p| Palette {
                sub_palette_id: p.palette_id,
                offset: u16::from(p.offset),
                length: u16::from(p.length),
            })
            .collect();
        corpse.properties.texture_maps = model
            .textures
            .iter()
            .map(|p| TextureMap {
                part_index: p.part_index,
                old_texture: p.old_texture,
                new_texture: p.new_texture,
            })
            .collect();
    }
    corpse
        .properties
        .instance_ids
        .retain(|p| ![1, 2, 3, 6, 18, 19].contains(&p.id));
    set(&mut corpse.properties.instance_ids, 18, actor.0);
    let generator = player
        .properties
        .instance_ids
        .iter()
        .find(|p| p.id == 6)
        .map(|p| p.value);
    let killer = killer.filter(|(id, _)| *id != actor && generator != Some(id.0));
    let killer_name = killer
        .map(|(_, name)| name)
        .filter(|name| !name.trim().is_empty())
        .map(|s| s.trim_start_matches('+'))
        .unwrap_or("misadventure");
    if let Some((id, _)) = killer {
        set(&mut corpse.properties.instance_ids, 19, id.0);
    }
    set(
        &mut corpse.properties.strings,
        1,
        format!(
            "{} of {}",
            if treasure { "Treasure" } else { "Corpse" },
            name.value
        ),
    );
    set(
        &mut corpse.properties.strings,
        16,
        format!("Killed by {killer_name}."),
    );
    set(&mut corpse.properties.ints, 6, 120);
    set(&mut corpse.properties.ints, 7, 10);
    corpse.properties.ints.retain(|p| ![10, 53].contains(&p.id));
    set(&mut corpse.properties.positions, 1, position);
    corpse
        .validate(bace_content::ContentLimits::default())
        .map_err(|e| e.to_string())?;
    Ok(corpse)
}

/// Project source corpse metadata after accepted item selection. Decay policy
/// belongs to bace-interactions; this adapter only writes its accepted result.
/// Timestamps outside the source Int32 representation are rejected explicitly.
pub fn freeze_corpse_metadata(
    corpse: &mut WeenieV1,
    level: u32,
    decay_seconds: u64,
    unix_seconds: i64,
    kind: bace_interactions::PlayerDeathKind,
) -> Result<(), String> {
    let timestamp = i32::try_from(unix_seconds).map_err(|_| "corpse timestamp overflow")?;
    if corpse.weenie_type != 14
        || !(1..=275).contains(&level)
        || timestamp < 0
        || !(1..=82500).contains(&decay_seconds)
    {
        return Err("invalid source corpse metadata".into());
    }
    set(&mut corpse.properties.ints, 25, level as i32);
    set(&mut corpse.properties.ints, 98, timestamp);
    set(&mut corpse.properties.floats, 44, decay_seconds as f64);
    corpse.properties.ints.retain(|p| p.id != 19);
    if kind == bace_interactions::PlayerDeathKind::Pk {
        set(&mut corpse.properties.ints, 99, 1);
    }
    Ok(())
}
