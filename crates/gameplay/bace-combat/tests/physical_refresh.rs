use bace_combat::preparation::*;
use bace_content::{BodyPart, Property, WeenieV1};
use bace_gameplay_api::weapon_combat::*;
use std::sync::Arc;
fn source() -> PreparedPhysicalRefreshSource {
    let mut actor = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "owner".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    actor.properties.ints = vec![Property { id: 307, value: 5 }];
    actor.properties.body_parts = vec![Property {
        id: 0,
        value: BodyPart {
            d_type: 4,
            d_val: 3,
            d_var: 0.2,
            llf: 1.,
            llb: 1.,
            lrf: 1.,
            lrb: 1.,
            mlf: 1.,
            mlb: 1.,
            mrf: 1.,
            mrb: 1.,
            hlf: 1.,
            hlb: 1.,
            hrf: 1.,
            hrb: 1.,
            ..Default::default()
        },
    }];
    let mut weapon = WeenieV1 {
        schema_version: 1,
        weenie_id: 2,
        class_name: "weapon".into(),
        weenie_type: 6,
        last_modified: None,
        properties: Default::default(),
    };
    weapon.properties.ints = vec![
        Property { id: 44, value: 20 },
        Property { id: 45, value: 4 },
        Property { id: 46, value: 2 },
        Property { id: 47, value: 4 },
        Property { id: 48, value: 44 },
    ];
    weapon.properties.floats = vec![Property { id: 62, value: 1.0 }];
    let actor = Arc::new(actor);
    let weapon = Arc::new(weapon);
    let skills = vec![
        (
            6,
            PhysicalSkill {
                advancement: 2,
                current: 100,
            },
        ),
        (
            44,
            PhysicalSkill {
                advancement: 3,
                current: 300,
            },
        ),
    ];
    let profile = prepare_physical(PhysicalPreparation {
        actor: 1,
        revision: 1,
        content_hash: [7; 32],
        player: true,
        weenie: &actor,
        equipment: &[PhysicalEquipment {
            entity: 100,
            revision: 1,
            location: 0x100000,
            weenie: &weapon,
        }],
        skills: &skills,
        attributes: [100; 6],
        base_attributes: [100; 6],
        qualities: &[],
        enchantments_complete: true,
        maneuvers: vec![PhysicalManeuver {
            style: 0x80000040,
            attack_type: 4,
            height: 2,
            minimum_skill: 0,
            motion: 0x10000063,
            duration: 0.2,
            hooks: vec![PhysicalAttackHook {
                seconds: 0.1,
                part: 0,
            }],
        }],
        height: 1.8,
        missile: None,
        melee_defense_modifier: 1.0,
        missile_defense_modifier: 1.0,
    })
    .unwrap();
    PreparedPhysicalRefreshSource {
        actor: 1,
        weenie: actor,
        equipment: vec![PhysicalEquipmentSource {
            entity: 100,
            revision: 1,
            location: 0x100000,
            weenie: weapon,
        }],
        qualities: vec![],
        profile: Arc::new(profile),
    }
}
fn projection(
    entity: u32,
    family: PhysicalQualityFamily,
    stat: u32,
    raw: f64,
    value: f64,
) -> PhysicalQualityProjection {
    PhysicalQualityProjection {
        entity,
        family,
        stat,
        part: None,
        details: PhysicalQuality {
            raw,
            increasing: 1.0,
            decreasing: 1.0,
            additive_increasing: value - raw,
            additive_decreasing: 0.0,
        },
        value,
    }
}
#[test]
fn registry_refresh_rebuilds_raw_weapon_and_ratings_without_compounding() {
    let source = source();
    let projections = [
        projection(100, PhysicalQualityFamily::Int, 44, 20.0, 30.0),
        projection(100, PhysicalQualityFamily::Float, 62, 1.0, 1.2),
        projection(1, PhysicalQualityFamily::Int, 307, 5.0, 15.0),
    ];
    let buffed = refresh_physical_from_source(&source, &source.profile, &projections).unwrap();
    assert_eq!(buffed.main.as_ref().unwrap().damage, 30.0);
    assert_eq!(buffed.main.as_ref().unwrap().offense, 1.2);
    assert_eq!(buffed.ratings.damage, 15);
    let refreshed = refresh_physical_from_source(&source, &buffed, &projections).unwrap();
    assert_eq!(refreshed, buffed);
    let expired = refresh_physical_from_source(&source, &buffed, &[]).unwrap();
    assert_eq!(expired.main.as_ref().unwrap().damage, 20.0);
    assert_eq!(expired.main.as_ref().unwrap().offense, 1.0);
    assert_eq!(expired.ratings.damage, 5);
    assert_eq!(expired.equipment, source.profile.equipment);
}
#[test]
fn stale_equipment_sources_and_nonfinite_registry_values_are_rejected() {
    let source = source();
    let mut current = (*source.profile).clone();
    current.equipment[0].revision = 2;
    assert!(refresh_physical_from_source(&source, &current, &[]).is_err());
    let value = projection(100, PhysicalQualityFamily::Int, 44, 20.0, f64::NAN);
    assert!(refresh_physical_from_source(&source, &source.profile, &[value]).is_err());
    assert_eq!(source.profile.main.as_ref().unwrap().damage, 20.0);
}
