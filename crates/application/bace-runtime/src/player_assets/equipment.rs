//! Cold rebuild after the equipment-effect owner has produced candidate registry
//! and item-XP state. Uses the same verified DAT and quality preparation as login.
use super::*;
use bace_simulation::{OwnedPlayerState, PreparedEquipmentPhysical};
use bace_storage_codec::{ItemPlacementV2, ItemSaveV4};
use bace_types::EntityId;
use sha2::{Digest, Sha256};
pub struct EquipmentPhysicalInput<'a> {
    pub actor: EntityId,
    pub before_revision: u64,
    pub source: &'a bace_content::WeenieV1,
    pub state: &'a OwnedPlayerState,
    pub items: &'a [ItemSaveV4],
    pub dat: &'a PreparedAvatarDat,
    pub projectile_shapes: &'a BTreeMap<u32, Arc<bace_physics::CollisionShape>>,
}
pub fn prepare_equipment_physical(
    input: EquipmentPhysicalInput<'_>,
) -> Result<PreparedEquipmentPhysical, String> {
    let state = input.state;
    prepare_equipment_physical_view(EquipmentPhysicalViewInput {
        previous_style: None,
        actor: input.actor,
        before_revision: input.before_revision,
        source: input.source,
        character: &state.character.progression,
        registry: state
            .enchantments
            .as_ref()
            .ok_or("equipment wearer registry missing")?,
        item_registries: state
            .item_enchantments
            .iter()
            .map(|(id, r)| (*id, r))
            .collect(),
        item_experience: &state.item_experience,
        items: input.items,
        dat: input.dat,
        projectile_shapes: input.projectile_shapes,
    })
}
pub struct EquipmentPhysicalViewInput<'a> {
    pub previous_style: Option<u32>,
    pub actor: EntityId,
    pub before_revision: u64,
    pub source: &'a bace_content::WeenieV1,
    pub character: &'a bace_character::CharacterProgression,
    pub registry: &'a bace_magic::EnchantmentRegistry,
    pub item_registries: Vec<(EntityId, &'a bace_magic::EnchantmentRegistry)>,
    pub item_experience: &'a [bace_simulation::PreparedItemExperience],
    pub items: &'a [ItemSaveV4],
    pub dat: &'a PreparedAvatarDat,
    pub projectile_shapes: &'a BTreeMap<u32, Arc<bace_physics::CollisionShape>>,
}
pub fn prepare_equipment_physical_view(
    input: EquipmentPhysicalViewInput<'_>,
) -> Result<PreparedEquipmentPhysical, String> {
    let EquipmentPhysicalViewInput {
        previous_style,
        actor,
        before_revision,
        source,
        character,
        registry,
        item_registries,
        item_experience,
        items,
        dat,
        projectile_shapes,
    } = input;
    if actor.0 == 0 || items.len() > 1023 || character.revision() != before_revision {
        return Err("equipment actor revision/capacity".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut equipped = Vec::new();
    for item in items {
        if !seen.insert(item.entity.object_id) {
            return Err("duplicate equipment inventory identity".into());
        }
        if let ItemPlacementV2::Contained {
            container,
            equipped: location,
            ..
        } = item.placement
            && location != 0
        {
            if container != actor.0 || equipped.len() >= 64 {
                return Err("equipment owner/capacity".into());
            }
            equipped.push((item, location));
        }
    }
    let mut gear_health = 0u32;
    let mut equipped_health = Vec::new();
    for (item, _) in &equipped {
        let health = item
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 379)
            .map_or(0, |p| p.value.max(0) as u32);
        gear_health = gear_health
            .checked_add(health)
            .ok_or("equipment gear health overflow")?;
        equipped_health.push((EntityId(item.entity.object_id), health));
    }
    let mut stats =
        super::stats::prepare_stats(source, character, registry, dat, gear_health, true)?;
    let equipment: Vec<_> = equipped
        .iter()
        .map(
            |(i, location)| bace_combat::preparation::PhysicalEquipment {
                entity: i.entity.object_id,
                revision: i.entity.mutation_revision,
                location: *location,
                weenie: &i.entity.state,
            },
        )
        .collect();
    let registries = equipped
        .iter()
        .map(|(i, _)| {
            let id = EntityId(i.entity.object_id);
            let registry = item_registries
                .iter()
                .find(|(owner, _)| *owner == id)
                .ok_or("equipment candidate registry missing")?
                .1;
            Ok((id, &i.entity.state, registry))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let qualities = prepare_player_physical_qualities(
        actor,
        source,
        registry,
        &registries,
        &dat.quality_filter,
    )?;
    let missile = super::missile::prepare_player_missile(
        dat,
        &equipment,
        &qualities,
        &stats,
        projectile_shapes,
    )?;
    let hash =
        Sha256::digest(bace_content_tools::compile_template(source).map_err(|e| e.to_string())?)
            .into();
    let profile =
        bace_combat::preparation::prepare_physical(bace_combat::preparation::PhysicalPreparation {
            actor: actor.0,
            revision: before_revision,
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
            height: dat
                .shape
                .nominal_height()
                .ok_or("equipment avatar nominal height")?,
            missile,
            melee_defense_modifier: 1.,
            missile_defense_modifier: 1.,
        })
        .map_err(|e| format!("equipment physical profile: {e:?}"))?;
    stats.skills.attack_skill = profile
        .main
        .as_ref()
        .or(profile.launcher.as_ref())
        .map_or(44, |w| w.skill);
    stats.skills.shield = profile
        .shield
        .as_ref()
        .map(|shield| bace_simulation::PreparedShield {
            effective_armor: shield.armor.raw as f32,
            magic_absorption: equipment
                .iter()
                .find(|e| e.location == 0x200000)
                .and_then(|e| e.weenie.properties.floats.iter().find(|p| p.id == 159))
                .map_or(0., |p| p.value as f32),
        });
    let ready = *dat
        .motions
        .style_defaults
        .get(&profile.style)
        .ok_or("equipment combat Ready motion missing")?;
    let scale = source
        .properties
        .floats
        .iter()
        .find(|p| p.id == 39)
        .map_or(1., |p| p.value as f32);
    let motions = crate::physical_assets::prepare_physical_motion_assets_with_previous_style(
        &profile,
        crate::physical_assets::PhysicalMotionAssets {
            table: &dat.motions,
            animations: &dat.animations,
            current_motion: ready,
            current_speed: 1.,
            scale,
            modifiers: &[],
        },
        previous_style,
    )?;
    let magic_equipment = equipment
        .iter()
        .map(|e| {
            Ok(bace_magic::MagicDamageEquipment {
                entity: e.entity,
                revision: e.revision,
                location: e.location,
                level: item_experience
                    .iter()
                    .find(|p| p.item.0 == e.entity)
                    .and_then(|p| p.experience)
                    .map(|xp| {
                        xp.level()
                            .map_err(|e| format!("equipment item level: {e:?}"))
                    })
                    .transpose()?,
                weenie: e.weenie,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let base_shield_skill = stats
        .skills
        .inputs
        .iter()
        .find(|(id, _)| *id == 48)
        .map(|(_, i)| {
            character
                .skill_values(48, *i)
                .map(|v| v.base)
                .map_err(|e| format!("equipment shield skill: {e:?}"))
        })
        .transpose()?
        .unwrap_or(0);
    let magic_damage =
        bace_magic::prepare_magic_damage_profile(bace_magic::MagicDamagePreparation {
            player: true,
            weenie: source,
            equipment: &magic_equipment,
            skills: &stats.physical_skills,
            base_attributes: stats.base_attributes,
            base_shield_skill,
        })
        .map_err(|e| format!("equipment magic defense profile: {e:?}"))?;
    let source = Arc::new(bace_combat::preparation::PreparedPhysicalRefreshSource {
        actor: actor.0,
        weenie: Arc::new(source.clone()),
        profile: Arc::new(profile),
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
        equipped_health,
    };
    let locomotion_styles = crate::physical_assets::prepare_locomotion_styles(
        &dat.motions,
        &dat.animations,
        source.profile.style,
        scale,
    )?;
    let death_motions = crate::physical_assets::prepare_death_motion_assets(
        &dat.motions,
        &dat.animations,
        &locomotion_styles,
        scale,
    )?;
    Ok(PreparedEquipmentPhysical {
        actor,
        before_revision,
        locomotion_styles,
        death_motions,
        source,
        motions,
        magic_damage,
        server_magic: bace_simulation::PreparedMagicAssetBatch {
            definitions: vec![],
            projectile_shapes: vec![],
        },
        skills: stats.skills,
        vital_inputs,
    })
}
