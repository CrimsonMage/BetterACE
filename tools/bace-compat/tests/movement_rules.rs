use bace_character::{JumpInput, RunInput, encumbrance, jump_proposal, run_rate};
use serde_json::Value;
fn u(v: &Value, key: &str) -> u32 {
    v[key].as_u64().unwrap() as u32
}
fn f(v: &Value, key: &str) -> f32 {
    v[key].as_f64().unwrap() as f32
}
fn bytes(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn official_csharp_run_burden_jump_and_combat_table() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/movement_rules.json")).unwrap();
    assert_eq!(
        fixture["commit"],
        "47edade3bd3f6044b676d4eb877c4965c7eda62b"
    );
    let vectors = &fixture["vectors"];
    for v in vectors["capacity"].as_array().unwrap() {
        let result =
            encumbrance(u(v, "strength"), u(v, "augs"), u64::from(u(v, "amount"))).unwrap();
        assert_eq!(result.capacity, u(v, "capacity"));
        assert_eq!(result.ratio, f(v, "burden"));
        assert_eq!(result.modifier, f(v, "modifier"));
    }
    for v in vectors["runs"].as_array().unwrap() {
        assert_eq!(
            run_rate(RunInput {
                skill: u(v, "skill"),
                burden: f(v, "burden"),
                scale: f(v, "scale"),
                exhausted: false
            })
            .unwrap(),
            f(v, "rate")
        );
    }
    for v in vectors["jumps"].as_array().unwrap() {
        let result = jump_proposal(JumpInput {
            skill: u(v, "skill"),
            burden: f(v, "burden"),
            scale: f(v, "scale"),
            extent: f(v, "power"),
            stamina: 10000,
            pk_timer_active: v["pk"].as_bool().unwrap(),
        })
        .unwrap();
        assert_eq!(result.height, f(v, "height"));
        assert_eq!(result.stamina_cost, u(v, "cost"));
    }
    let vector = &vectors["cmt"];
    let bytes = bytes(vector["bytes"].as_str().unwrap());
    let table = bace_dat::CombatManeuverTable::decode(&bytes).unwrap();
    assert_eq!(table.id, u(vector, "id"));
    assert_eq!(
        table.maneuvers.len(),
        vector["rows"].as_array().unwrap().len()
    );
    for (m, row) in table
        .maneuvers
        .iter()
        .zip(vector["rows"].as_array().unwrap())
    {
        assert_eq!(
            [
                m.style,
                m.attack_height,
                m.attack_type,
                m.min_skill_level,
                m.motion
            ],
            std::array::from_fn(|i| row[i].as_u64().unwrap() as u32)
        );
    }
    for n in 0..bytes.len() {
        assert!(bace_dat::CombatManeuverTable::decode(&bytes[..n]).is_err());
    }
}
#[test]
fn official_interpreter_direction_hold_and_rate_modifiers() {
    use bace_geometry::Vec3;
    use bace_motion::{LocomotionControls, LocomotionProfile, MotionPhysics};
    let v = |x, y, z| MotionPhysics {
        velocity: Vec3::new(x, y, 0.),
        omega: Vec3::new(0., 0., z),
    };
    let profile = LocomotionProfile {
        style: 0x8000003d,
        ready: v(0., 0., 0.),
        walk: v(0., 3.12, 0.),
        run: v(0., 4., 0.),
        sidestep: v(1.25, 0., 0.),
        turn: v(0., 0., 1.),
    };
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/movement_rules.json")).unwrap();
    for value in fixture["vectors"]["controls"].as_array().unwrap() {
        let command = u(value, "input");
        let speed = f(value, "speed");
        let mut controls = LocomotionControls {
            run: value["run"].as_bool().unwrap(),
            ..Default::default()
        };
        match command {
            0x45000005 => controls.forward = speed,
            0x45000006 => controls.forward = -speed,
            0x6500000d => controls.turn = speed,
            0x6500000e => controls.turn = -speed,
            0x6500000f => controls.sidestep = speed,
            0x65000010 => controls.sidestep = -speed,
            _ => panic!(),
        }
        let drive = profile.interpret(controls, f(value, "rate")).unwrap();
        let actual = match command {
            0x45000005 | 0x45000006 => {
                assert_eq!(drive.forward_motion, u(value, "motion"));
                drive.forward_rate
            }
            0x6500000d | 0x6500000e => drive.turn_rate,
            _ => drive.side_rate,
        };
        assert_eq!(actual, f(value, "adjusted"), "{value}");
    }
}
fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            if let (Some(a), Some(b)) = (a.as_u64(), b.as_u64()) {
                a == b
            } else {
                a.as_f64().map(|v| v as f32) == b.as_f64().map(|v| v as f32)
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| b.get(key).is_some_and(|b| equivalent(value, b)))
        }
        _ => a == b,
    }
}
fn frame_value(f: &bace_dat::ModelFrame) -> Value {
    serde_json::json!({"origin":f.origin,"rotation":f.rotation})
}
fn location_value(p: &bace_dat::ContractLocation) -> Value {
    serde_json::json!({"cell":p.cell,"frame":frame_value(&p.frame)})
}
#[test]
fn official_contracts_landblock_info_and_land_height_projection() {
    use serde_json::json;
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/movement_rules.json")).unwrap();
    let vectors = &fixture["vectors"];
    let vector = &vectors["contracts"];
    let data = bytes(vector["bytes"].as_str().unwrap());
    let table = bace_dat::ContractTable::decode(&data).unwrap();
    assert_eq!(table.bucket_size, 7);
    for (key, c) in table.contracts {
        let actual = json!({"version":c.version,"id":c.id,"name":c.name,"description":c.description,"progress_description":c.progress_description,"start_npc":c.start_npc,"end_npc":c.end_npc,"stamped_quest":c.stamped_quest,"started_quest":c.started_quest,"finished_quest":c.finished_quest,"progress_quest":c.progress_quest,"timer_quest":c.timer_quest,"repeat_time_quest":c.repeat_time_quest,"start_location":location_value(&c.start_location),"end_location":location_value(&c.end_location),"quest_location":location_value(&c.quest_location)});
        assert!(equivalent(&actual, &vector["rows"][key.to_string()]));
    }
    for n in 0..data.len() {
        assert!(bace_dat::ContractTable::decode(&data[..n]).is_err());
    }
    let vector = &vectors["land_info"];
    let data = bytes(vector["bytes"].as_str().unwrap());
    let info = bace_dat::LandblockInfo::decode(&data).unwrap();
    assert_eq!(info.id, u(vector, "id"));
    assert_eq!(info.cells, u(vector, "cells"));
    assert_eq!(info.pack_mask, u(vector, "pack_mask") as u16);
    assert_eq!(info.restriction_bucket_size, Some(8));
    let objects: Vec<_> = info
        .objects
        .iter()
        .map(|o| json!({"id":o.id,"frame":frame_value(&o.frame)}))
        .collect();
    assert!(equivalent(&json!(objects), &vector["objects"]));
    let buildings:Vec<_>=info.buildings.iter().map(|b|json!({"model":b.model,"frame":frame_value(&b.frame),"leaves":b.leaves,"portals":b.portals.iter().map(|p|json!({"flags":p.flags,"other_cell":p.other_cell,"other_portal":p.other_portal,"visible_cells":p.visible_cells})).collect::<Vec<_>>()})).collect();
    assert!(equivalent(&json!(buildings), &vector["buildings"]));
    assert!(equivalent(
        &json!(info.restrictions),
        &vector["restrictions"]
    ));
    for n in 0..data.len() {
        assert!(bace_dat::LandblockInfo::decode(&data[..n]).is_err());
    }
    let vector = &vectors["land"];
    let data = bytes(vector["bytes"].as_str().unwrap());
    let land = bace_dat::RegionLand::decode_prefix(&data).unwrap();
    assert_eq!(land.region_number, 1);
    assert_eq!(land.version, 2);
    assert_eq!(land.name, "Dereth");
    let actual = json!({"length":land.block_length,"width":land.block_width,"square":land.square_length,"block":land.landblock_length,"vertices":land.vertices_per_cell,"max_height":land.maximum_object_height,"sky_height":land.sky_height,"road_width":land.road_width,"heights":land.heights.to_vec()});
    let mut expected = vector.clone();
    expected.as_object_mut().unwrap().remove("bytes");
    assert!(equivalent(&actual, &expected));
    for n in 0..data.len() {
        assert!(bace_dat::RegionLand::decode_prefix(&data[..n]).is_err());
    }
}
#[test]
fn official_sequence_root_displacement_and_cyclic_frame_progression() {
    use bace_geometry::Vec3;
    use bace_motion::{MotionDrive, RootCursor, RootCycle, RootFrame, RootSegment};
    let segments = (0..2)
        .map(|s| RootSegment {
            low: if s == 0 { 1 } else { 0 },
            high: 3 - s,
            frame_count: (4 - s) as u32,
            framerate: if s == 0 { 30.0 } else { 15.0 },
            frames: (0..4 - s)
                .map(|i| RootFrame {
                    translation: Vec3::new(i as f32 * 0.001, 0.13 + i as f32 * 0.01, 0.0),
                    heading: i as f32 * 0.001,
                })
                .collect(),
        })
        .collect();
    let cycle = RootCycle::prepare(segments).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/movement_rules.json")).unwrap();
    let drive = MotionDrive {
        local_velocity: Vec3::new(1.0, 2.0, 0.0),
        angular_velocity: 0.3,
        forward_motion: 0,
        forward_rate: 1.0,
        side_rate: 0.0,
        turn_rate: 0.0,
    };
    for case in fixture["vectors"]["root_motion"].as_array().unwrap() {
        let mut cursor = RootCursor::default();
        for (tick, expected) in case["steps"].as_array().unwrap().iter().enumerate() {
            let result = cycle
                .advance(&mut cursor, f(case, "dt"), f(case, "speed"), drive)
                .unwrap();
            for (i, actual) in [
                result.translation.x,
                result.translation.y,
                result.translation.z,
            ]
            .into_iter()
            .enumerate()
            {
                assert!(
                    (actual - expected["translation"][i].as_f64().unwrap() as f32).abs() < 0.00005,
                    "dt={} speed={} tick={tick} component={i} actual={actual} expected={expected}",
                    case["dt"],
                    case["speed"]
                );
            }
            assert!((result.heading - f(expected, "heading")).abs() < 0.00005);
            assert_eq!(cursor.phase().0, u(expected, "segment") as usize);
            assert_eq!(cursor.phase().1, f(expected, "frame"));
        }
    }
}
