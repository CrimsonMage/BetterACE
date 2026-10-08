//! Cold source loot generation for Player_Death.CalculateDeathItems_Olthoi.
use super::*;
use bace_simulation::OlthoiDeathKind;
use std::sync::Arc;
pub struct OlthoiLootInput<'a> {
    pub kind: OlthoiDeathKind,
    pub player: &'a PlayerSaveV6,
    pub snapshot: &'a bace_simulation::PlayerReadSnapshot,
    pub generation: &'a PackGeneration,
    pub assets: Arc<bace_loot::TreasureAssets>,
    pub aetheria_rate: f32,
    pub root: Arc<bace_random::RandomRoot>,
    pub epoch: u64,
    pub operation: u64,
    pub unix_seconds: i32,
}
pub struct PreparedOlthoiLoot {
    pub kind: OlthoiDeathKind,
    pub had_vitae: bool,
    pub before_timestamp: Option<i32>,
    pub after_timestamp: Option<i32>,
    /// Source ordered roots and descendants. Parent indices always precede children.
    pub sources: Vec<bace_loot::PreparedContainerItem>,
}
pub fn prepare_olthoi_loot(input: OlthoiLootInput<'_>) -> Result<PreparedOlthoiLoot, String> {
    let had_vitae = input
        .snapshot
        .enchantments()
        .ok_or("death registry missing")?
        .entries()
        .iter()
        .any(|e| e.spell == 666);
    let state = crate::player_death_state::restore_player_death_state(input.player)
        .map_err(|e| e.to_string())?;
    let before_timestamp = state.olthoi_loot_timestamp;
    let mut result = PreparedOlthoiLoot {
        kind: input.kind,
        had_vitae,
        before_timestamp,
        after_timestamp: before_timestamp,
        sources: vec![],
    };
    if had_vitae || input.kind == OlthoiDeathKind::Empty {
        return Ok(result);
    }
    let mut identity = [0; 16];
    identity[..8].copy_from_slice(&input.epoch.to_le_bytes());
    identity[8..].copy_from_slice(&input.operation.to_le_bytes());
    let mut random = input
        .root
        .event_stream(identity, bace_random::Domain::PlayerDeath)
        .and_then(|s| s.fork(b"olthoi", u64::from(input.snapshot.binding().actor.0)))
        .map_err(|e| format!("death random: {e:?}"))?;
    if input.kind == OlthoiDeathKind::Slag {
        let level = u32::try_from(int(&input.player.player.entity.state, 25).unwrap_or(0))
            .map_err(|_| "negative death level")?;
        if let Some(slag) = bace_loot::roll_player_slag(
            level,
            had_vitae,
            before_timestamp.unwrap_or(0),
            input.unix_seconds,
            &mut random,
        )
        .map_err(|e| format!("slag: {e:?}"))?
        {
            let mut source = template(input.generation, 43491)?;
            bace_loot::set_treasure_stack(&mut source, slag.stack as i32)
                .map_err(|e| format!("slag stack: {e:?}"))?;
            result.sources.push(bace_loot::PreparedContainerItem {
                source,
                source_destination: None,
                parent_index: None,
                generator_parent_index: None,
                inventory_slot: 0,
            });
            result.after_timestamp = Some(slag.timestamp);
        }
    } else {
        let profile = death_profile(input.generation)?;
        let treasure =
            bace_loot::DeathTreasure::prepare(profile, input.assets.clone(), input.aetheria_rate)
                .map_err(|e| format!("Olthoi treasure assets: {e:?}"))?;
        let mut roots = treasure
            .generate(&mut random)
            .map_err(|e| format!("Olthoi treasure: {e:?}"))?;
        if bace_loot::roll_player_gland(had_vitae, &mut random)
            .map_err(|e| format!("gland: {e:?}"))?
        {
            roots.push(template(input.generation, 43747)?);
        }
        for root in roots {
            let offset = result.sources.len();
            let tree = bace_loot::materialize_container_tree(root, &input.assets.templates)
                .map_err(|e| format!("Olthoi contained treasure: {e:?}"))?;
            if offset.checked_add(tree.len()).is_none_or(|n| n > 120) {
                return Err("Olthoi corpse source capacity".into());
            }
            result.sources.extend(tree.into_iter().map(|mut node| {
                node.parent_index = node.parent_index.map(|parent| parent + offset);
                node.generator_parent_index =
                    node.generator_parent_index.map(|parent| parent + offset);
                node
            }));
        }
    }
    if result.sources.len() > 120 {
        return Err("Olthoi corpse source capacity".into());
    }
    Ok(result)
}
fn death_profile(generation: &PackGeneration) -> Result<bace_content::TreasureDeathRowV1, String> {
    let mut cursor = Some(PackKey {
        namespace: 39,
        id: 0,
    });
    let mut count = 0;
    let mut found = None;
    'scan: loop {
        let batch = generation.scan(cursor, 256).map_err(|e| e.to_string())?;
        if batch.is_empty() {
            break;
        }
        for (key, row) in batch {
            cursor = Some(key);
            if key.namespace != 39 {
                break 'scan;
            }
            count += 1;
            if count > 65536 {
                return Err("death treasure row bound".into());
            }
            let PackLookup::Record(row) = row else {
                continue;
            };
            if row.bytes().len() > 4096 {
                return Err("death treasure row byte bound".into());
            }
            let bace_content::WorldRecordV1::TreasureDeath(profile) =
                bace_content_tools::decode_world_record(row.bytes())?
            else {
                return Err("invalid death treasure namespace".into());
            };
            if profile.treasure_type == 2222 {
                if found.is_some() {
                    return Err("duplicate Olthoi treasure profile".into());
                }
                found = Some(profile);
            }
        }
    }
    Ok(found.unwrap_or(bace_content::TreasureDeathRowV1 {
        id: 2222,
        treasure_type: 2222,
        tier: 8,
        loot_quality_mod: 0.,
        unknown_chances: 19,
        item_chance: 100,
        item_min_amount: 1,
        item_max_amount: 2,
        item_treasure_type_selection_chances: 8,
        magic_item_chance: 100,
        magic_item_min_amount: 2,
        magic_item_max_amount: 3,
        magic_item_treasure_type_selection_chances: 8,
        mundane_item_chance: 100,
        mundane_item_min_amount: 0,
        mundane_item_max_amount: 1,
        mundane_item_type_selection_chances: 7,
        last_modified: String::new(),
    }))
}
