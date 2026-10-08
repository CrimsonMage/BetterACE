//! Join saved ownership and verified assets into one simulation admission aggregate.
use super::*;
use bace_simulation::{MagicCaster, PreparedPlayerAdmission};
use bace_types::EntityId;
use sha2::{Digest, Sha256};
pub struct PlayerColdPolicy {
    pub account_created_unix: Option<i64>,
    pub staff: bace_gameplay_api::staff::StaffRegistration,
    pub portal_access: bace_interactions::PortalAccess,
    pub components_required: bool,
    pub safe_components: bool,
}
pub struct PlayerColdAssets<'a> {
    pub avatar: &'a PreparedAvatarDat,
    pub geometry: &'a bace_physics::GeometryRegion,
    pub spell_rows: &'a BTreeMap<u32, bace_content::SpellRowV1>,
    pub component_templates: &'a [u32],
    pub projectile_shapes: &'a BTreeMap<u32, Arc<bace_physics::CollisionShape>>,
}
pub fn prepare_loaded_avatar(
    loaded: &crate::game_login::LoadedPlayer,
    assets: PlayerColdAssets<'_>,
    policy: PlayerColdPolicy,
    now_unix_millis: u64,
) -> Result<PreparedPlayerAdmission, String> {
    let source = &loaded.player.player.entity.state;
    let actor = loaded.binding.actor;
    let dat = assets.avatar;
    // A frozen companion is constructor evidence, not an admitted Creature owner.
    // Keep the valuable tree in the durable load result until that owner is wired.
    if loaded.inventory.iter().any(|item| {
        item.construction.is_some()
            || matches!(item.entity.state.weenie_type, 10 | 12 | 15 | 61 | 69 | 71)
    }) {
        return Err("constructed creature reconstruction is not yet admitted".into());
    }
    let mut item_xp = Vec::new();
    for (index, item) in loaded.inventory.iter().enumerate() {
        item_xp.push(crate::item_experience::prepare_item_experience(
            actor,
            EntityId(item.entity.object_id),
            item.entity.mutation_revision,
            index as u64 + 1,
            &item.entity.state,
            &dat.spells,
            assets.spell_rows,
        )?);
    }
    let set_values = equipment_sets(loaded, &item_xp)?;
    let sets: Vec<_> = set_values
        .iter()
        .map(|(id, (possible, active))| bace_magic::EquippedSpellSet {
            id: *id,
            possible,
            active,
        })
        .collect();
    let mut state = crate::player_saves::restore_player_at_with_item_experience(
        loaded,
        &dat.character,
        now_unix_millis,
        assets.component_templates,
        &sets,
        &item_xp,
        |id| player_enchantment_definition(id, &dat.spells, assets.spell_rows),
    )
    .map_err(|e| e.to_string())?;
    state.equipment_mana = Some(crate::equipment_mana::prepare_equipment_mana(
        actor,
        source,
        &loaded
            .inventory
            .iter()
            .map(|item| (EntityId(item.entity.object_id), &item.entity.state))
            .collect::<Vec<_>>(),
        &dat.spells,
    )?);
    for item in &loaded.inventory {
        let id = EntityId(item.entity.object_id);
        if !state
            .item_enchantments
            .iter()
            .any(|(owner, _)| *owner == id)
        {
            state.item_enchantments.push((
                id,
                bace_magic::EnchantmentRegistry::new(4096)
                    .map_err(|e| format!("item registry: {e:?}"))?,
            ));
        }
    }
    if let Some(mana) = &mut state.equipment_mana {
        crate::equipment_mana::prepare_equipment_mana_cleanup(
            actor,
            mana,
            state
                .enchantments
                .as_ref()
                .ok_or("player mana cleanup registry")?,
            &state.item_enchantments,
        )?;
    }
    let equipped:Vec<_>=loaded.inventory.iter().filter(|i|matches!(i.placement,bace_storage_codec::ItemPlacementV2::Contained{container,equipped,..}if container==actor.0 && equipped!=0)).collect();
    let gear_health = equipped
        .iter()
        .try_fold(0u32, |n, item| {
            n.checked_add(integer(&item.entity.state, 379, 0).max(0) as u32)
        })
        .ok_or("gear health overflow")?;
    let mut stats = prepare_player_stats(source, &state, dat, gear_health)?;
    let inventory = prepare_player_inventory(loaded, &state, &stats)?;
    let equipment: Vec<_> = equipped
        .iter()
        .map(|item| {
            let bace_storage_codec::ItemPlacementV2::Contained { equipped, .. } = item.placement
            else {
                unreachable!()
            };
            bace_combat::preparation::PhysicalEquipment {
                entity: item.entity.object_id,
                revision: item.entity.mutation_revision,
                location: equipped,
                weenie: &item.entity.state,
            }
        })
        .collect();
    let item_registries = equipped
        .iter()
        .map(|item| {
            let id = EntityId(item.entity.object_id);
            let registry = &state
                .item_enchantments
                .iter()
                .find(|(owner, _)| *owner == id)
                .ok_or("missing item registry")?
                .1;
            Ok((id, &item.entity.state, registry))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let registry = state.enchantments.as_ref().ok_or("player registry")?;
    let qualities = prepare_player_physical_qualities(
        actor,
        source,
        registry,
        &item_registries,
        &dat.quality_filter,
    )?;
    let hash =
        Sha256::digest(bace_content_tools::compile_template(source).map_err(|e| e.to_string())?)
            .into();
    let missile = super::missile::prepare_player_missile(
        dat,
        &equipment,
        &qualities,
        &stats,
        assets.projectile_shapes,
    )?;
    let profile =
        bace_combat::preparation::prepare_physical(bace_combat::preparation::PhysicalPreparation {
            actor: actor.0,
            revision: state.character.progression.revision(),
            content_hash: hash,
            player: true,
            weenie: source,
            equipment: &equipment,
            skills: &stats.physical_skills,
            attributes: stats.current_attributes,
            base_attributes: stats.base_attributes,
            qualities: &qualities,
            enchantments_complete: true,
            maneuvers: dat.maneuvers.clone(),
            height: dat.shape.nominal_height().ok_or("avatar nominal height")?,
            missile,
            melee_defense_modifier: 1.,
            missile_defense_modifier: 1.,
        })
        .map_err(|e| format!("player physical preparation: {e:?}"))?;
    stats.skills.attack_skill = profile
        .main
        .as_ref()
        .or(profile.launcher.as_ref())
        .map_or(44, |w| w.skill);
    if let Some(shield) = &profile.shield {
        stats.skills.shield = Some(bace_simulation::PreparedShield {
            effective_armor: shield.armor.raw as f32,
            magic_absorption: equipment
                .iter()
                .find(|e| e.location == 0x200000)
                .map_or(0., |e| floating(e.weenie, 159, 0.) as f32),
        });
    }
    let maneuver = profile
        .maneuvers
        .iter()
        .find(|m| m.style == profile.style)
        .ok_or("avatar has no accepted combat maneuver")?;
    let mut combatant = bace_entity::Combatant::new(bace_entity::CombatantProfile {
        maximum_health: stats.vitals[0].maximum,
        melee_damage: profile.main.as_ref().map_or_else(
            || profile.body_attacks.first().map_or(0, |b| b.damage as u32),
            |w| w.damage as u32,
        ),
        melee_range: profile.range,
        attack_duration: maneuver.duration,
        strike_offsets: maneuver.hooks.iter().map(|h| h.seconds).collect(),
        player: true,
    })
    .map_err(|e| format!("avatar combatant: {e:?}"))?
    .with_resources(Some(stats.vitals[1]), Some(stats.vitals[2]))
    .map_err(|e| format!("avatar resources: {e:?}"))?;
    combatant
        .damage(stats.vitals[0].maximum - stats.vitals[0].current)
        .map_err(|e| format!("avatar health: {e:?}"))?;
    let current = |id| {
        stats
            .physical_skills
            .iter()
            .find(|(skill, _)| *skill == id)
            .map(|(_, s)| s.current)
            .ok_or("required avatar skill missing")
    };
    let caster = MagicCaster {
        player: true,
        known_spells: state
            .character
            .ui
            .as_ref()
            .ok_or("avatar UI")?
            .known_spells
            .iter()
            .copied()
            .collect(),
        school_skills: [
            current(34)?,
            current(33)?,
            current(31)?,
            current(32)?,
            current(43)?,
        ],
        magic_defense: current(15)?,
        mana_conversion: current(16)?,
        components_required: policy.components_required,
        safe_components: policy.safe_components,
    };
    let burden = inventory
        .items
        .iter()
        .try_fold(0u64, |n, i| {
            n.checked_add(u64::from(i.stack) * u64::from(i.unit_burden))
        })
        .ok_or("avatar burden overflow")?;
    let encumbrance = bace_character::encumbrance(
        stats.current_attributes[0],
        integer(source, 230, 0).max(0) as u32,
        burden,
    )
    .map_err(|e| format!("avatar encumbrance: {e:?}"))?;
    let scale = floating(source, 39, 1.) as f32;
    let run = bace_character::run_rate(bace_character::RunInput {
        skill: current(24)?,
        burden: encumbrance.ratio,
        scale,
        exhausted: stats.vitals[1].current == 0,
    })
    .map_err(|e| format!("avatar run: {e:?}"))?;
    // Admission stores an upper bound; every accepted jump still needs the live
    // stamina/PK/extent proposal at movement ingress before physical application.
    let jump = bace_character::jump_proposal(bace_character::JumpInput {
        skill: if stats.vitals[1].current == 0 {
            0
        } else {
            current(22)?
        },
        burden: encumbrance.ratio,
        scale: 1.,
        extent: 1.,
        stamina: u32::MAX,
        pk_timer_active: false,
    })
    .map_err(|e| format!("avatar jump bounds: {e:?}"))?;
    let position = &source
        .properties
        .positions
        .iter()
        .find(|p| p.id == 1)
        .ok_or("avatar location")?
        .value;
    let spawn = bace_physics::GeometrySpawn {
        cell: position.obj_cell_id,
        position: bace_geometry::Vec3::new(
            position.position_x,
            position.position_y,
            position.position_z,
        ),
        shape: dat.shape.clone(),
        capabilities: bace_motion::Capabilities {
            speed: dat.locomotion.profile.run.velocity.length_squared().sqrt() * run,
            jump_impulse: (jump.height * 19.6).sqrt(),
        },
        heading: 2. * position.rotation_z.atan2(position.rotation_w),
        maximum_turn_rate: dat.locomotion.profile.turn.omega.z.abs() * 1.5,
    };
    let presence =
        super::presence::prepare_presence(loaded, &state, policy.staff.privileges.account_access)?;
    let now_seconds = i64::try_from(now_unix_millis / 1000).map_err(|_| "chat clock overflow")?;
    let account_15_days = source
        .properties
        .bools
        .iter()
        .any(|p| p.id == 127 && p.value)
        || policy
            .account_created_unix
            .and_then(|created| now_seconds.checked_sub(created))
            .is_some_and(|age| age >= 15 * 86400);
    let chat_eligibility = bace_social::ChatEligibility {
        account_created_unix: policy.account_created_unix,
        account_15_days,
        player_age_seconds: u64::try_from(
            source
                .properties
                .ints
                .iter()
                .find(|p| p.id == 125)
                .map_or(0, |p| p.value),
        )
        .map_err(|_| "invalid character age")?,
    };
    let properties = super::presence::prepare_properties(source, &stats)?;
    let magic_damage = super::prepare_player_magic_damage(loaded, &state, &stats)?;
    let physical_source = Arc::new(bace_combat::preparation::PreparedPhysicalRefreshSource {
        actor: actor.0,
        weenie: Arc::new(source.clone()),
        profile: Arc::new(profile.clone()),
        qualities,
        equipment: equipment
            .iter()
            .map(|e| bace_combat::preparation::PhysicalEquipmentSource {
                entity: e.entity,
                revision: e.revision,
                location: e.location,
                weenie: Arc::new(e.weenie.clone()),
            })
            .collect(),
    });
    let vital_inputs = bace_simulation::PreparedVitalInputs {
        formulas: [dat.vitals.health, dat.vitals.stamina, dat.vitals.mana].map(|f| {
            bace_character::VitalFormula {
                enabled: f.x != 0,
                divisor: f.z,
                attribute1: f.attribute1,
                attribute2: f.attribute2,
            }
        }),
        equipped_health: equipped
            .iter()
            .map(|item| {
                (
                    EntityId(item.entity.object_id),
                    integer(&item.entity.state, 379, 0).max(0) as u32,
                )
            })
            .collect(),
    };
    let physical_motions = crate::physical_assets::prepare_physical_motion_assets(
        &profile,
        crate::physical_assets::PhysicalMotionAssets {
            table: &dat.motions,
            animations: &dat.animations,
            current_motion: 0x41000003,
            current_speed: 1.0,
            scale,
            modifiers: &[],
        },
    )?;
    let locomotion_styles = crate::physical_assets::prepare_locomotion_styles(
        &dat.motions,
        &dat.animations,
        profile.style,
        scale,
    )?;
    let death_motions = crate::physical_assets::prepare_death_motion_assets(
        &dat.motions,
        &dat.animations,
        &locomotion_styles,
        scale,
    )?;
    crate::player_preparation::prepare_player_admission(
        loaded,
        state,
        assets.geometry,
        spawn,
        crate::player_preparation::PlayerAdmissionCapabilities {
            locomotion: dat.locomotion.clone(),
            locomotion_styles,
            death_motions,
            properties,
            combatant,
            caster,
            magic_damage,
            vital_inputs,
            physical_motions,
            physical_source,
            physical: Arc::new(profile),
            skills: stats.skills,
            items: inventory.items,
            containers: inventory.containers,
            presence,
            chat_eligibility,
            staff: policy.staff,
            portal_access: policy.portal_access,
        },
    )
}
fn integer(w: &bace_content::WeenieV1, id: u32, default: i32) -> i32 {
    w.properties
        .ints
        .iter()
        .find(|p| p.id == id)
        .map_or(default, |p| p.value)
}
fn floating(w: &bace_content::WeenieV1, id: u32, default: f64) -> f64 {
    w.properties
        .floats
        .iter()
        .find(|p| p.id == id)
        .map_or(default, |p| p.value)
}
type EquipmentSetSpells = BTreeMap<u32, (Vec<u32>, Vec<u32>)>;
fn equipment_sets(
    loaded: &crate::game_login::LoadedPlayer,
    prepared: &[bace_simulation::PreparedItemExperience],
) -> Result<EquipmentSetSpells, String> {
    let mut groups = BTreeMap::<u32, Vec<&bace_simulation::PreparedItemExperience>>::new();
    for item in &loaded.inventory {
        if matches!(item.placement,bace_storage_codec::ItemPlacementV2::Contained{container,equipped,..}if container==loaded.binding.actor.0 && equipped!=0)
        {
            let p = prepared
                .iter()
                .find(|p| p.item.0 == item.entity.object_id)
                .ok_or("set item metadata")?;
            if let Some(set) = &p.set {
                groups.entry(set.id).or_default().push(p);
            }
        }
    }
    let mut result = BTreeMap::new();
    for (id, items) in groups {
        let set = items[0].set.as_ref().ok_or("equipment set")?;
        let score = if items[0].set_uses_item_levels {
            items.iter().try_fold(0u32, |n, p| -> Result<u32, String> {
                let level = p
                    .experience
                    .map(|v| v.level())
                    .transpose()
                    .map_err(|e| format!("item level: {e:?}"))?
                    .unwrap_or(0);
                n.checked_add(level)
                    .ok_or_else(|| "set score overflow".into())
            })?
        } else {
            items.len() as u32
        };
        let possible = set
            .tiers
            .values()
            .flatten()
            .map(|p| p.entry.spell)
            .collect();
        let active = set
            .tiers
            .range(..=score)
            .next_back()
            .map_or(Vec::new(), |(_, entries)| {
                entries.iter().map(|p| p.entry.spell).collect()
            });
        result.insert(id, (possible, active));
    }
    Ok(result)
}
