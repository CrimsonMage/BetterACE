use bace_dat::{
    BspNode, BspTree, BspTreeKind, BuildingInfo, BuildingPortal, CellGeometry, EnvCell,
    Environment, Landblock, LandblockInfo, ModelFrame, ModelVertex, RegionLand,
};
use bace_gameplay_api::staff::MapTeleportRequest;
use bace_runtime::staff_map::prepare_map_destination;
use std::collections::BTreeMap;
fn land() -> RegionLand {
    RegionLand {
        region_number: 1,
        version: 1,
        name: "fixture".into(),
        block_length: 255,
        block_width: 255,
        square_length: 24.,
        landblock_length: 8,
        vertices_per_cell: 1,
        maximum_object_height: 100.,
        sky_height: 1000.,
        road_width: 1.,
        heights: [42.; 256],
    }
}
fn frame() -> ModelFrame {
    ModelFrame {
        origin: [0.; 3],
        rotation: [1., 0., 0., 0.],
    }
}
fn geometry(z: f32) -> CellGeometry {
    let tree = BspTree {
        kind: BspTreeKind::Cell,
        nodes: vec![BspNode {
            tag: 0x4c454146,
            splitting_plane: None,
            positive: None,
            negative: None,
            sphere: None,
            polygons: vec![],
            portal_polygons: vec![],
            leaf_index: Some(0),
            solid: None,
        }],
    };
    CellGeometry {
        vertices: BTreeMap::from([(
            0,
            ModelVertex {
                position: [0., 0., z],
                normal: [0., 0., 1.],
                uvs: vec![],
            },
        )]),
        polygons: BTreeMap::new(),
        portals: vec![],
        cell_bsp: tree.clone(),
        physics_polygons: BTreeMap::new(),
        physics_bsp: tree,
        drawing_bsp: None,
    }
}
fn building() -> LandblockInfo {
    LandblockInfo {
        id: 0x1234fffe,
        cells: 1,
        objects: vec![],
        pack_mask: 0,
        buildings: vec![BuildingInfo {
            model: 0x01000001,
            frame: frame(),
            leaves: 1,
            portals: vec![BuildingPortal {
                flags: 0,
                other_cell: 0x101,
                other_portal: 0,
                visible_cells: vec![],
            }],
        }],
        restriction_bucket_size: None,
        restrictions: BTreeMap::new(),
    }
}
fn cells() -> BTreeMap<u32, EnvCell> {
    BTreeMap::from([(
        0x12340101,
        EnvCell {
            id: 0x12340101,
            flags: 0,
            repeated_id: 0x12340101,
            surfaces: vec![],
            environment_id: 0x0d000001,
            cell_structure: 0,
            position: frame(),
            portals: vec![],
            visible_cells: vec![],
            static_objects: vec![],
            restriction_object: None,
        },
    )])
}
fn environments() -> BTreeMap<u32, Environment> {
    BTreeMap::from([(
        0x0d000001,
        Environment {
            id: 0x0d000001,
            cells: BTreeMap::from([(0, geometry(4.))]),
        },
    )])
}
fn request() -> MapTeleportRequest {
    MapTeleportRequest {
        cell: 0x12340001,
        origin: [10., 20., 9999.],
        rotation: [1., 0., 0., 0.],
    }
}
#[test]
fn independent_original_handler_map_vectors_match_asset_preparation() {
    for row in include_str!("../../../network/bace-wire/tests/fixtures/staff_map.csv")
        .lines()
        .skip(2)
    {
        let fields: Vec<_> = row.split(',').collect();
        let water = fields[1] == "true";
        let has_building = fields[2] == "true";
        let block = Landblock {
            id: 0x1234ffff,
            has_objects: has_building,
            terrain: [if water { 16 << 2 } else { 0 }; 81],
            heights: [0; 81],
        };
        let info = has_building.then(building);
        let output = prepare_map_destination(
            request(),
            7,
            &block,
            &land(),
            info.as_ref(),
            &cells(),
            &environments(),
        )
        .unwrap();
        assert_eq!(output.entirely_water, water);
        if fields[5] == "true" {
            assert_eq!(output.destination.cell, fields[6].parse::<u32>().unwrap());
            assert_eq!(
                output.destination.origin[2],
                fields[7].parse::<f32>().unwrap()
            );
        }
        assert_ne!(output.destination.origin[2], 9999.);
    }
}
#[test]
fn missing_assets_nonfinite_click_and_partial_water_never_use_reported_height() {
    let mut block = Landblock {
        id: 0x1234ffff,
        has_objects: true,
        terrain: [16 << 2; 81],
        heights: [0; 81],
    };
    block.terrain[80] = 0;
    assert!(
        !prepare_map_destination(
            request(),
            7,
            &block,
            &land(),
            Some(&building()),
            &cells(),
            &environments()
        )
        .unwrap()
        .entirely_water
    );
    assert!(
        prepare_map_destination(
            request(),
            7,
            &block,
            &land(),
            None,
            &cells(),
            &environments()
        )
        .is_err()
    );
    assert!(
        prepare_map_destination(
            request(),
            7,
            &block,
            &land(),
            Some(&building()),
            &BTreeMap::new(),
            &environments()
        )
        .is_err()
    );
    let mut bad = request();
    bad.origin[0] = f32::NAN;
    assert!(
        prepare_map_destination(
            bad,
            7,
            &block,
            &land(),
            Some(&building()),
            &cells(),
            &environments()
        )
        .is_err()
    );
    let mut env = environments();
    env.get_mut(&0x0d000001)
        .unwrap()
        .cells
        .insert(1, geometry(-2.));
    // Source GetMinZ scans every structure, so negative secondary geometry disables the addition.
    assert_eq!(
        prepare_map_destination(
            request(),
            7,
            &block,
            &land(),
            Some(&building()),
            &cells(),
            &env
        )
        .unwrap()
        .destination
        .origin[2],
        42.
    );
}
