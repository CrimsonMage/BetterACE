use bace_dat::{BspTree, BspTreeKind, EnvCell};
use serde_json::{Value, json};
fn bytes(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}
fn frame(f: &bace_dat::ModelFrame) -> Value {
    json!({"origin":f.origin,"rotation":f.rotation})
}
#[test]
fn official_csharp_envcell_and_every_bsp_branch_match_all_exposed_fields() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/dat_geometry.json")).unwrap();
    assert_eq!(
        fixture["commit"],
        "47edade3bd3f6044b676d4eb877c4965c7eda62b"
    );
    for vector in fixture["vectors"]["env"].as_array().unwrap() {
        let b = bytes(vector["bytes"].as_str().unwrap());
        let e = EnvCell::decode(&b).unwrap();
        let actual = json!({"id":e.id,"flags":e.flags,"surfaces":e.surfaces,"environment_id":e.environment_id,"cell_structure":e.cell_structure,"position":frame(&e.position),"portals":e.portals.iter().map(|p|json!({"flags":p.flags,"polygon_id":p.polygon_id,"other_cell_id":p.other_cell_id,"other_portal_id":p.other_portal_id})).collect::<Vec<_>>(),"visible_cells":e.visible_cells,"static_objects":e.static_objects.iter().map(|s|json!({"id":s.id,"frame":frame(&s.frame)})).collect::<Vec<_>>(),"restriction_object":e.restriction_object});
        assert!(equivalent(&actual, &vector["decoded"]));
        assert_eq!(e.repeated_id, 0x98760100);
        for n in 0..b.len() {
            assert!(EnvCell::decode(&b[..n]).is_err());
        }
    }
    for vector in fixture["vectors"]["bsp"].as_array().unwrap() {
        let kind = match vector["kind"].as_u64().unwrap() {
            0 => BspTreeKind::Drawing,
            1 => BspTreeKind::Physics,
            2 => BspTreeKind::Cell,
            _ => panic!("kind"),
        };
        let b = bytes(vector["bytes"].as_str().unwrap());
        let tree = BspTree::decode(&b, kind).unwrap();
        let nodes:Vec<_>=tree.nodes.iter().map(|n|json!({"tag":n.tag,"positive":n.positive,"negative":n.negative,"splitting_plane":n.splitting_plane,"sphere":n.sphere.as_ref().map(|s|json!({"origin":s.origin,"radius":s.radius})),"polygons":n.polygons,"portal_polygons":n.portal_polygons,"leaf_index":n.leaf_index,"solid":n.solid})).collect();
        assert!(
            equivalent(&json!(nodes), &vector["nodes"]),
            "{kind:?} {}",
            vector["tag"]
        );
        for n in 0..b.len() {
            assert!(BspTree::decode(&b[..n], kind).is_err());
        }
    }
}

fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            if a.is_f64() || b.is_f64() {
                a.as_f64().map(|v| v as f32) == b.as_f64().map(|v| v as f32)
            } else {
                a == b
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| equivalent(a, b)))
        }
        _ => a == b,
    }
}
fn motion_data(d: &bace_dat::MotionData) -> Value {
    json!({"bitfield":d.bitfield,"flags":d.flags,"animations":d.animations.iter().map(|a|json!({"animation_id":a.animation_id,"low_frame":a.low_frame,"high_frame":a.high_frame,"framerate":a.framerate})).collect::<Vec<_>>(),"velocity":d.velocity,"omega":d.omega})
}
fn motion_map(m: &std::collections::BTreeMap<u32, bace_dat::MotionData>) -> Value {
    Value::Object(
        m.iter()
            .map(|(k, v)| (k.to_string(), motion_data(v)))
            .collect(),
    )
}
fn vector(v: [f32; 3]) -> Value {
    json!({"X":v[0],"Y":v[1],"Z":v[2]})
}
fn hook(h: &bace_dat::AnimationHook) -> Value {
    use bace_dat::AnimationHookPayload as P;
    let mut v = json!({"HookType":h.kind,"Direction":h.direction});
    let m = v.as_object_mut().unwrap();
    let values = match &h.payload {
        P::Empty => json!({}),
        P::Id(id) => {
            let name = match h.kind {
                1 => "Id",
                2 => "SoundType",
                14 | 15 => "EmitterId",
                16 => "NoDraw",
                18 => "PartIndex",
                _ => panic!("unexpected ID hook"),
            };
            let mut o = serde_json::Map::new();
            o.insert(name.into(), json!(id));
            Value::Object(o)
        }
        P::State(s) => {
            let mut o = serde_json::Map::new();
            o.insert(
                if h.kind == 6 { "Ethereal" } else { "LightsOn" }.into(),
                json!(s),
            );
            Value::Object(o)
        }
        P::Attack(c) => {
            json!({"AttackCone":{"PartIndex":c.part_index,"LeftX":c.left[0],"LeftY":c.left[1],"RightX":c.right[0],"RightY":c.right[1],"Radius":c.radius,"Height":c.height}})
        }
        P::ReplaceObject {
            raw_part_index,
            part_index,
            object_id,
        } => {
            assert_eq!(*raw_part_index, 0x1234);
            json!({"APChange":{"PartIndex":part_index,"PartID":object_id}})
        }
        P::Transition {
            part,
            start,
            end,
            time,
        } => {
            let mut v = json!({"Start":start,"End":end,"Time":time});
            if let Some(part) = part {
                v["Part"] = json!(part);
            }
            v
        }
        P::Scale { end, time } => json!({"End":end,"Time":time}),
        P::Particle {
            emitter_info_id,
            part_index,
            offset,
            emitter_id,
        } => {
            json!({"EmitterInfoId":emitter_info_id,"PartIndex":part_index,"EmitterId":emitter_id,"Offset":{"Origin":vector(offset.origin),"Orientation":{"IsIdentity":offset.rotation==[1.0,0.0,0.0,0.0],"W":offset.rotation[0],"X":offset.rotation[1],"Y":offset.rotation[2],"Z":offset.rotation[3]}}})
        }
        P::CallPes { pes, pause } => json!({"PES":pes,"Pause":pause}),
        P::SoundTweaked {
            sound_id,
            priority,
            probability,
            volume,
        } => {
            json!({"SoundID":sound_id,"Priority":priority,"Probability":probability,"Volume":volume})
        }
        P::Omega(axis) => json!({"Axis":vector(*axis)}),
        P::TextureVelocity { part, uv } => {
            let mut v = json!({"USpeed":uv[0],"VSpeed":uv[1]});
            if let Some(part) = part {
                v["PartIndex"] = json!(part);
            }
            v
        }
    };
    m.extend(values.as_object().unwrap().clone());
    v
}
#[test]
fn official_motion_tables_animation_frames_and_all_known_hook_payloads_match() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/dat_geometry.json")).unwrap();
    let vectors = &fixture["vectors"];
    let raw = bytes(vectors["motion"]["bytes"].as_str().unwrap());
    let m = bace_dat::MotionTable::decode(&raw).unwrap();
    let links: serde_json::Map<String, Value> = m
        .links
        .iter()
        .map(|(k, v)| (k.to_string(), motion_map(v)))
        .collect();
    let actual = json!({"id":m.id,"default_style":m.default_style,"style_defaults":m.style_defaults,"cycles":motion_map(&m.cycles),"modifiers":motion_map(&m.modifiers),"links":links});
    assert!(equivalent(&actual, &vectors["motion"]["decoded"]));
    for n in 0..raw.len() {
        assert!(bace_dat::MotionTable::decode(&raw[..n]).is_err());
    }
    let raw = bytes(vectors["animation"]["bytes"].as_str().unwrap());
    let a = bace_dat::Animation::decode(&raw).unwrap();
    let actual = json!({"id":a.id,"flags":a.flags,"num_parts":a.num_parts,"num_frames":a.num_frames,"position_frames":a.position_frames.iter().map(frame).collect::<Vec<_>>(),"frames":a.frames.iter().map(|f|json!({"parts":f.parts.iter().map(frame).collect::<Vec<_>>(),"hooks":f.hooks.iter().map(hook).collect::<Vec<_>>()})).collect::<Vec<_>>()});
    assert!(equivalent(&actual, &vectors["animation"]["decoded"]));
    assert_eq!(
        a.attack_hooks()
            .map(|(frame, direction, cone)| (frame, direction, cone.part_index))
            .collect::<Vec<_>>(),
        vec![(0, -1, 7), (1, -1, 7)]
    );
    for n in 0..raw.len() {
        assert!(bace_dat::Animation::decode(&raw[..n]).is_err());
    }
}
fn bsp(tree: &bace_dat::BspTree) -> Value {
    json!(tree.nodes.iter().map(|n|json!({"tag":n.tag,"positive":n.positive,"negative":n.negative,"splitting_plane":n.splitting_plane,"sphere":n.sphere.as_ref().map(|s|json!({"origin":s.origin,"radius":s.radius})),"polygons":n.polygons,"portal_polygons":n.portal_polygons,"leaf_index":n.leaf_index,"solid":n.solid})).collect::<Vec<_>>())
}
fn polys(polygons: &std::collections::BTreeMap<u16, bace_dat::ModelPolygon>) -> Value {
    Value::Object(polygons.iter().map(|(id,p)|(id.to_string(),json!({"vertices":p.vertices,"positive_uvs":p.positive_uvs,"negative_uvs":p.negative_uvs,"positive_surface":p.positive_surface,"negative_surface":p.negative_surface,"cull":p.cull,"stippling":p.stippling}))).collect())
}
#[test]
fn official_environment_vertices_polygons_portals_and_three_bsp_kinds_match() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/dat_geometry.json")).unwrap();
    let vector = &fixture["vectors"]["environment"];
    let raw = bytes(vector["bytes"].as_str().unwrap());
    let e = bace_dat::Environment::decode(&raw).unwrap();
    assert_eq!(json!(e.id), vector["id"]);
    let cells:serde_json::Map<String,Value>=e.cells.iter().map(|(id,c)|{
  let vertices:serde_json::Map<String,Value>=c.vertices.iter().map(|(id,v)|(id.to_string(),json!({"position":v.position,"normal":v.normal,"uvs":v.uvs}))).collect();
  (id.to_string(),json!({"vertices":vertices,"polygons":polys(&c.polygons),"portals":c.portals,"cell_bsp":bsp(&c.cell_bsp),"physics_polygons":polys(&c.physics_polygons),"physics_bsp":bsp(&c.physics_bsp),"drawing_bsp":c.drawing_bsp.as_ref().map(bsp)}))
 }).collect();
    assert!(equivalent(&json!(cells), &vector["cells"]));
    for n in 0..raw.len() {
        assert!(bace_dat::Environment::decode(&raw[..n]).is_err());
    }
}
fn sphere(s: &bace_dat::BspSphere) -> Value {
    json!({"origin":s.origin,"radius":s.radius})
}
#[test]
fn official_setup_collision_primitives_placements_and_limits_are_preserved() {
    let f: Value = serde_json::from_str(include_str!("../fixtures/dat_geometry.json")).unwrap();
    let v = &f["vectors"]["setup"];
    let raw = bytes(v["bytes"].as_str().unwrap());
    let s = bace_dat::CollisionSetup::decode(&raw).unwrap();
    let holding: serde_json::Map<String, Value> = s
        .holding_locations
        .iter()
        .map(|(id, l)| {
            (
                id.to_string(),
                json!({"part_id":l.part_id,"frame":frame(&l.frame)}),
            )
        })
        .collect();
    let connection: serde_json::Map<String, Value> = s
        .connection_points
        .iter()
        .map(|(id, l)| {
            (
                id.to_string(),
                json!({"part_id":l.part_id,"frame":frame(&l.frame)}),
            )
        })
        .collect();
    let placements:serde_json::Map<String,Value>=s.placements.iter().map(|(id,p)|(id.to_string(),json!({"parts":p.parts.iter().map(frame).collect::<Vec<_>>(),"hooks":p.hooks.iter().map(hook).collect::<Vec<_>>()}))).collect();
    let lights:serde_json::Map<String,Value>=s.lights.iter().map(|(id,l)|(id.to_string(),json!({"frame":frame(&l.frame),"color":l.color,"intensity":l.intensity,"falloff":l.falloff,"cone_angle":l.cone_angle}))).collect();
    let actual = json!({"id":s.id,"flags":s.flags,"parts":s.parts,"parents":s.parents,"default_scales":s.default_scales,"holding_locations":holding,"connection_points":connection,"placements":placements,"cylinders":s.cylinders.iter().map(|c|json!({"origin":c.origin,"radius":c.radius,"height":c.height})).collect::<Vec<_>>(),"spheres":s.spheres.iter().map(sphere).collect::<Vec<_>>(),"height":s.height,"radius":s.radius,"step_up_height":s.step_up_height,"step_down_height":s.step_down_height,"sorting_sphere":sphere(&s.sorting_sphere),"selection_sphere":sphere(&s.selection_sphere),"lights":lights,"default_animation":s.default_animation,"default_script":s.default_script,"default_motion_table":s.default_motion_table,"default_sound_table":s.default_sound_table,"default_script_table":s.default_script_table});
    assert!(equivalent(&actual, &v["decoded"]));
    for n in 0..raw.len() {
        assert!(bace_dat::CollisionSetup::decode(&raw[..n]).is_err());
    }
}
