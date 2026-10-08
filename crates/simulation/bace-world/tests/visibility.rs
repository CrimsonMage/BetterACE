use bace_entity::Actor;
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_types::{CellId, EntityId};
use bace_world::{PreparedCellVisibility, VisibilityError, World, visibility_distance_squared};
fn add(world: &mut World, id: u32, cell: u32, position: Vec3) {
    let cell = CellId(cell);
    if world.scene(cell).is_err() {
        world
            .register_scene(
                cell,
                SyntheticScene::new(
                    -1000.0,
                    Aabb::new(
                        Vec3::new(-2000.0, -2000.0, -2000.0),
                        Vec3::new(2000.0, 2000.0, 20000.0),
                    )
                    .unwrap(),
                    vec![],
                )
                .unwrap(),
            )
            .unwrap();
    }
    let body = Body::spawn(
        world.scene(cell).unwrap(),
        position,
        0.1,
        Capabilities {
            speed: 0.0,
            jump_impulse: 0.0,
        },
    )
    .unwrap();
    world
        .insert(Actor {
            id: EntityId(id),
            cell,
            body,
        })
        .unwrap();
}
fn metadata() -> Vec<PreparedCellVisibility> {
    vec![
        PreparedCellVisibility {
            cell: CellId(0x10100100),
            seen_outside: false,
            visible_cells: vec![CellId(0x10100102)].into(),
        },
        PreparedCellVisibility {
            cell: CellId(0x10100101),
            seen_outside: true,
            visible_cells: vec![CellId(0x10100100)].into(),
        },
        PreparedCellVisibility {
            cell: CellId(0x10100102),
            seen_outside: false,
            visible_cells: vec![].into(),
        },
    ]
}
#[test]
fn outdoor_and_indoor_pvs_preserve_seen_outside_and_one_level_visible_cells() {
    let mut world = World::default();
    world.install_cell_visibility(metadata()).unwrap();
    let targets = [
        0x10100001, 0x11100001, 0x12100001, 0x10100100, 0x10100101, 0x10100102,
    ];
    for (i, cell) in targets.into_iter().enumerate() {
        add(
            &mut world,
            2 + i as u32,
            cell,
            Vec3::new(i as f32, 0.0, 0.5),
        );
    }
    add(&mut world, 1, 0x10100001, Vec3::new(100., 0., 0.5));
    let mut output = Vec::new();
    world
        .visibility_candidates(EntityId(1), &mut output, 32)
        .unwrap();
    assert_eq!(
        output.iter().map(|c| c.entity.0).collect::<Vec<_>>(),
        vec![2, 3, 6]
    );
    world
        .teleport(EntityId(1), CellId(0x10100100), Vec3::new(100., 0., 0.5))
        .unwrap();
    world
        .visibility_candidates(EntityId(1), &mut output, 32)
        .unwrap();
    assert_eq!(
        output.iter().map(|c| c.entity.0).collect::<Vec<_>>(),
        vec![5, 7]
    );
    world
        .teleport(EntityId(1), CellId(0x10100101), Vec3::new(100., 0., 0.5))
        .unwrap();
    world
        .visibility_candidates(EntityId(1), &mut output, 32)
        .unwrap();
    assert_eq!(
        output.iter().map(|c| c.entity.0).collect::<Vec<_>>(),
        vec![2, 3, 5, 6]
    );
    assert!(
        !output.iter().any(|c| c.entity == EntityId(7)),
        "PVS is not transitive through visible rooms"
    );
}
#[test]
fn accepted_teleport_retirement_and_projectile_cells_invalidate_the_id_index() {
    let mut world = World::default();
    add(&mut world, 1, 0x10100001, Vec3::ZERO);
    add(&mut world, 2, 0x11100001, Vec3::new(1., 0., 0.5));
    add(&mut world, 3, 0x12100001, Vec3::new(2., 0., 0.5));
    let mut output = Vec::new();
    world
        .visibility_candidates(EntityId(1), &mut output, 32)
        .unwrap();
    assert_eq!(output.len(), 1);
    world
        .teleport(EntityId(2), CellId(0x12100001), Vec3::new(1., 0., 0.5))
        .unwrap();
    world
        .visibility_candidates(EntityId(1), &mut output, 32)
        .unwrap();
    assert!(output.is_empty());
    world
        .insert_projectile(
            EntityId(100),
            bace_world::OwnedProjectile {
                cell: CellId(0x10100001),
                source: EntityId(1),
                target: None,
                body: bace_physics::ProjectileBody::new(
                    Vec3::new(10., 0., 1.),
                    Vec3::ZERO,
                    0.1,
                    0.0,
                    1.0,
                )
                .unwrap(),
            },
        )
        .map_err(|(e, _)| e)
        .unwrap();
    world
        .visibility_candidates(EntityId(1), &mut output, 32)
        .unwrap();
    assert_eq!(output[0].entity, EntityId(100));
    world.remove_projectile(EntityId(100));
    world
        .visibility_candidates(EntityId(1), &mut output, 32)
        .unwrap();
    assert!(output.is_empty());
}
#[test]
fn invalid_or_missing_pvs_is_atomic_and_query_capacity_never_returns_a_partial_set() {
    let mut world = World::default();
    let mut rows = metadata();
    rows[1].visible_cells = vec![CellId(0x20200100)].into();
    assert_eq!(
        world.install_cell_visibility(rows),
        Err(VisibilityError::InvalidMetadata)
    );
    assert!(world.cell_visibility(CellId(0x10100100)).is_none());
    assert_eq!(
        world.install_cell_visibility(vec![metadata().remove(0)]),
        Err(VisibilityError::MissingCell)
    );
    add(&mut world, 1, 0x10100001, Vec3::ZERO);
    add(&mut world, 2, 0x10100001, Vec3::new(1., 0., 0.5));
    add(&mut world, 3, 0x10100001, Vec3::new(2., 0., 0.5));
    let mut output = Vec::new();
    assert_eq!(
        world.visibility_candidates(EntityId(1), &mut output, 1),
        Err(VisibilityError::Capacity)
    );
    assert!(output.is_empty());
    add(&mut world, 4, 0x10100100, Vec3::new(3., 0., 0.5));
    assert_eq!(
        world.visibility_candidates(EntityId(1), &mut output, 32),
        Err(VisibilityError::MissingCell)
    );
    assert!(output.is_empty());
}
#[test]
fn source_initial_distance_is_2d_and_accounts_for_landblock_offsets_without_wrap() {
    assert_eq!(
        visibility_distance_squared(
            CellId(0x10100001),
            Vec3::new(190., 10., 0.),
            CellId(0x11100001),
            Vec3::new(1., 10., 10000.)
        )
        .unwrap(),
        9.0
    );
    assert_eq!(
        visibility_distance_squared(CellId(1), Vec3::ZERO, CellId(0xff000001), Vec3::ZERO).unwrap(),
        (255.0f32 * 192.0).powi(2)
    );
}
#[test]
fn independent_original_ace_pvs_and_distance_vectors() {
    let bits = |value: &str| f32::from_bits(u32::from_str_radix(value, 16).unwrap());
    let cell = |value: &str| CellId(u32::from_str_radix(value, 16).unwrap());
    let mut count = 0;
    for row in include_str!("fixtures/visibility.csv")
        .lines()
        .filter(|r| !r.starts_with('#'))
    {
        let v: Vec<_> = row.split(',').collect();
        if v[0] == "distance" {
            let actual = visibility_distance_squared(
                cell(v[1]),
                Vec3::new(bits(v[2]), bits(v[3]), 10000.0),
                cell(v[4]),
                Vec3::new(bits(v[5]), bits(v[6]), -10000.0),
            )
            .unwrap();
            assert_eq!(actual.to_bits(), bits(v[7]).to_bits(), "{row}");
        } else {
            let mut world = World::default();
            world.install_cell_visibility(metadata()).unwrap();
            for (index, target) in [
                0x10100001, 0x11100001, 0x12100001, 0x10100100, 0x10100101, 0x10100102,
            ]
            .into_iter()
            .enumerate()
            {
                add(
                    &mut world,
                    index as u32 + 2,
                    target,
                    Vec3::new(index as f32, 0.0, 0.5),
                );
            }
            add(&mut world, 1, cell(v[1]).0, Vec3::new(100.0, 0.0, 0.5));
            let mut output = Vec::new();
            world
                .visibility_candidates(EntityId(1), &mut output, 32)
                .unwrap();
            let expected: Vec<u32> = v[2].split(';').map(|id| id.parse().unwrap()).collect();
            assert_eq!(
                output.iter().map(|c| c.entity.0).collect::<Vec<_>>(),
                expected,
                "{row}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 30);
}

#[test]
#[ignore = "representative bounded visibility-query cost sample; prints local timing"]
fn indexed_visibility_cost_for_two_thousand_actors_and_128_observers() {
    let mut world = World::default();
    for block in 0..16u32 {
        for slot in 0..125u32 {
            let id = ((block * 125 + slot) * 1919) % 2000 + 1;
            let cell = ((0x40 + block / 4) << 24) | ((0x40 + block % 4) << 16) | 1;
            add(
                &mut world,
                id,
                cell,
                Vec3::new((slot % 11) as f32 * 12., (slot / 11) as f32 * 12., 0.5),
            );
        }
    }
    let mut output = Vec::with_capacity(2000);
    let storage = output.as_ptr();
    // Warm index membership once; queries then touch only adjacent cell buckets.
    world
        .visibility_candidates(EntityId(1), &mut output, 2000)
        .unwrap();
    let start = std::time::Instant::now();
    let mut candidates = 0usize;
    for _ in 0..10 {
        for observer in 1..=128 {
            world
                .visibility_candidates(EntityId(observer), &mut output, 2000)
                .unwrap();
            candidates += output.len();
        }
    }
    assert_eq!(output.as_ptr(), storage, "caller buffer is reused");
    eprintln!(
        "2000 actors /128 observers /10 passes: {:?}, {} candidate views, fixed output capacity {}",
        start.elapsed(),
        candidates,
        output.capacity()
    );
}

#[test]
fn launch_captures_reverse_pvs_and_exact_accepted_view_before_projectile_removal() {
    use bace_gameplay_api::{CharacterBinding, SessionId};
    use bace_types::AccountId;
    let mut world = World::default();
    world.install_cell_visibility(metadata()).unwrap();
    add(&mut world, 1, 0x10100100, Vec3::new(1., 2., 3.));
    add(&mut world, 2, 0x10100102, Vec3::new(5., 2., 3.));
    add(&mut world, 3, 0x20200001, Vec3::new(5., 2., 3.));
    let body = bace_physics::ProjectileBody::new(
        Vec3::new(5., 2., 3.),
        Vec3::new(8., 0., 2.),
        0.1,
        -9.8,
        5.,
    )
    .unwrap();
    world
        .insert_projectile(
            EntityId(99),
            bace_world::OwnedProjectile {
                cell: CellId(0x10100102),
                source: EntityId(1),
                target: Some(EntityId(2)),
                body,
            },
        )
        .unwrap_or_else(|_| panic!("projectile insertion"));
    let snapshot = world
        .projectile_launch_snapshot(EntityId(99), 7, |id| {
            (id.0 <= 3).then_some(CharacterBinding {
                session: SessionId(id.0 as u64),
                account: AccountId(id.0 as u64),
                actor: id,
            })
        })
        .unwrap();
    assert_eq!(
        snapshot
            .observers
            .iter()
            .map(|o| o.binding.actor.0)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(snapshot.observers[0].distance_squared, 16.);
    world.remove_projectile(EntityId(99));
    assert_eq!(snapshot.view.position, [5., 2., 3.]);
    assert_eq!(snapshot.view.velocity, [8., 0., 2.]);
    assert_eq!(snapshot.tick, 7);
}
