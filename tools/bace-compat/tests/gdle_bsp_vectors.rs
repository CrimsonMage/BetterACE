use bace_geometry::Vec3;
use bace_physics::{
    BspCollisionNode, BspLimits, BspQueryBudget, CollisionPlane, CollisionSphere, GdleBspCell,
    GdlePolygon, sphere_path_steps,
};
use serde_json::Value;
fn vec(v: &Value) -> Vec3 {
    Vec3::new(
        v[0].as_f64().unwrap() as f32,
        v[1].as_f64().unwrap() as f32,
        v[2].as_f64().unwrap() as f32,
    )
}
fn cell() -> GdleBspCell {
    let polygon = GdlePolygon::prepare(vec![
        Vec3::new(-2.0, -2.0, 0.0),
        Vec3::new(2.0, -2.0, 0.0),
        Vec3::new(2.0, 2.0, 0.0),
        Vec3::new(-2.0, 2.0, 0.0),
    ])
    .unwrap();
    let bounds = CollisionSphere {
        center: Vec3::ZERO,
        radius: 10.0,
    };
    let plane = polygon.plane();
    GdleBspCell::prepare(
        vec![
            BspCollisionNode::Branch {
                bounds,
                plane,
                positive: 1,
                negative: 2,
            },
            BspCollisionNode::Leaf {
                bounds,
                solid: false,
                polygons: vec![0],
            },
            BspCollisionNode::Leaf {
                bounds,
                solid: true,
                polygons: vec![0],
            },
        ],
        vec![polygon],
        BspLimits::default(),
    )
    .unwrap()
}
#[test]
fn pinned_gdle_scalar_polygon_bsp_contact_and_subdivision_vectors() {
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/gdle_bsp.json")).unwrap();
    assert_eq!(
        fixture["commit"],
        "353cbab52ef7da2b7063bc3e3f008461d8531693"
    );
    let cell = cell();
    let plane = CollisionPlane {
        normal: vec(&fixture["vectors"]["plane"]["normal"]),
        distance: fixture["vectors"]["plane"]["distance"].as_f64().unwrap() as f32,
    };
    assert_eq!(plane.normal, Vec3::new(0.0, 0.0, 1.0));
    for (index, v) in fixture["vectors"]["contacts"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let sphere = CollisionSphere {
            center: vec(&v["center"]),
            radius: v["radius"].as_f64().unwrap() as f32,
        };
        let movement = vec(&v["movement"]);
        assert_eq!(
            cell.intersects_solid(sphere, true, &mut BspQueryBudget::default())
                .unwrap(),
            v["solid_center"].as_bool().unwrap(),
            "center case{index}"
        );
        assert_eq!(
            cell.intersects_solid(sphere, false, &mut BspQueryBudget::default())
                .unwrap(),
            v["solid_surface"].as_bool().unwrap(),
            "surface case{index}"
        );
        let contact = cell
            .sphere_contact(sphere, movement, &mut BspQueryBudget::default())
            .unwrap();
        if v["contact"].is_null() {
            assert!(contact.is_none(), "case{index}");
        } else {
            let c = contact.unwrap_or_else(|| panic!("missing case{index}"));
            assert_eq!(c.point, vec(&v["contact"]["point"]));
            assert_eq!(
                c.approaching,
                v["contact"]["approaching"].as_bool().unwrap()
            );
            assert_eq!(c.plane, plane);
        }
    }
    for v in fixture["vectors"]["steps"].as_array().unwrap() {
        let (count, step) = sphere_path_steps(
            CollisionSphere {
                center: Vec3::ZERO,
                radius: v["radius"].as_f64().unwrap() as f32,
            },
            vec(&v["end"]),
            1024,
        )
        .unwrap();
        assert_eq!(count, v["count"].as_u64().unwrap() as u32);
        assert_eq!(step, vec(&v["step"]));
    }
}
fn prepare_actual_cell(
    cell: &bace_dat::CellGeometry,
) -> Result<GdleBspCell, bace_physics::BspQueryError> {
    use bace_physics::BspQueryError as E;
    let mut polygons = Vec::new();
    let mut indices = std::collections::BTreeMap::new();
    for (id, p) in &cell.physics_polygons {
        let vertices = p
            .vertices
            .iter()
            .map(|id| {
                let v = cell.vertices.get(id).ok_or(E::InvalidGeometry)?.position;
                Ok(Vec3::new(v[0], v[1], v[2]))
            })
            .collect::<Result<Vec<_>, E>>()?;
        indices.insert(*id, polygons.len());
        polygons.push(GdlePolygon::prepare(vertices)?);
    }
    let mut nodes = Vec::new();
    for n in &cell.physics_bsp.nodes {
        let s = n.sphere.as_ref().ok_or(E::InvalidGeometry)?;
        let bounds = CollisionSphere {
            center: Vec3::new(s.origin[0], s.origin[1], s.origin[2]),
            radius: s.radius,
        };
        nodes.push(if n.leaf_index.is_some() {
            BspCollisionNode::Leaf {
                bounds,
                solid: n.solid.ok_or(E::InvalidGeometry)? != 0,
                polygons: n
                    .polygons
                    .iter()
                    .map(|id| indices.get(id).copied().ok_or(E::InvalidGeometry))
                    .collect::<Result<Vec<_>, _>>()?,
            }
        } else {
            let plane = n.splitting_plane.ok_or(E::InvalidGeometry)?;
            BspCollisionNode::Branch {
                bounds,
                plane: CollisionPlane {
                    normal: Vec3::new(plane[0], plane[1], plane[2]),
                    distance: plane[3],
                },
                positive: n.positive.ok_or(E::InvalidGeometry)?,
                negative: n.negative.ok_or(E::InvalidGeometry)?,
            }
        });
    }
    GdleBspCell::prepare(nodes, polygons, BspLimits::default())
}
#[test]
#[ignore = "requires approved user-supplied portal DAT; set BACE_DAT_DIRECTORY"]
fn supplied_real_dungeon_geometry_preparation_and_bounded_queries() {
    let root = std::path::PathBuf::from(
        std::env::var_os("BACE_DAT_DIRECTORY").expect("BACE_DAT_DIRECTORY required"),
    );
    let path = root.join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut dat = bace_dat::DatArchive::open(path).unwrap();
    let ids: Vec<_> = dat
        .records()
        .keys()
        .copied()
        .filter(|id| id >> 24 == 0x0d)
        .collect();
    let mut prepared = 0;
    let mut observations = 0;
    for id in ids {
        let env = bace_dat::Environment::decode(&dat.read(id).unwrap()).unwrap();
        for (cell_id, c) in env.cells {
            let cell = prepare_actual_cell(&c)
                .unwrap_or_else(|e| panic!("prepare {id:08x}/{cell_id}: {e}"));
            prepared += 1;
            for point in [
                Vec3::ZERO,
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(-1.0, -1.0, 0.5),
            ] {
                let sphere = CollisionSphere {
                    center: point,
                    radius: 0.5,
                };
                cell.intersects_solid(sphere, true, &mut BspQueryBudget::default())
                    .unwrap();
                cell.sphere_contact(
                    sphere,
                    Vec3::new(0.0, 0.0, -0.1),
                    &mut BspQueryBudget::default(),
                )
                .unwrap();
                observations += 1;
            }
        }
    }
    eprintln!("actual environment cells prepared={prepared}, bounded query pairs={observations}");
    assert_eq!(prepared, 3168);
    assert_eq!(observations, 9504);
}
#[test]
fn pinned_empty_leaf_bounds_are_uninterpreted_when_no_polygons_exist() {
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/gdle_bsp.json")).unwrap();
    let filler = f32::from_bits(0xcdcdcdcd);
    let bounds = CollisionSphere {
        center: Vec3::new(filler, filler, filler),
        radius: filler,
    };
    let cell = GdleBspCell::prepare(
        vec![BspCollisionNode::Leaf {
            bounds,
            solid: true,
            polygons: vec![],
        }],
        vec![],
        BspLimits::default(),
    )
    .unwrap();
    let sphere = CollisionSphere {
        center: Vec3::ZERO,
        radius: 0.5,
    };
    let v = &fixture["vectors"]["empty_leaf"];
    assert_eq!(
        cell.intersects_solid(sphere, true, &mut BspQueryBudget::default())
            .unwrap(),
        v["solid"].as_bool().unwrap()
    );
    let contact = cell
        .sphere_contact(
            sphere,
            Vec3::new(0.0, 0.0, -1.0),
            &mut BspQueryBudget::default(),
        )
        .unwrap();
    assert_eq!(contact.is_some(), v["has_contact"].as_bool().unwrap());
    assert_eq!(
        contact.is_some_and(|c| c.approaching),
        v["approaching"].as_bool().unwrap()
    );
}
