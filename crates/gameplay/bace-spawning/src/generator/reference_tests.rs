//! Differential fixture values emitted by compiled unchanged official ACE methods.
use super::*;
fn profile(probability: f32, init_create: i32, max_create: i32) -> GeneratorProfile {
    GeneratorProfile {
        id: 0,
        probability,
        weenie_class_id: 10,
        delay: None,
        init_create,
        max_create,
        when_create: 1,
        where_create: 0,
        stack_size: None,
        palette_id: None,
        shade: None,
        position: Default::default(),
    }
}
fn clock() -> GeneratorClock {
    GeneratorClock {
        tick: 1,
        unix_seconds: 1,
        is_day: true,
        event: GeneratorEventState::Missing,
    }
}
fn definition(
    profiles: Vec<GeneratorProfile>,
    initial_count: i32,
    maximum_count: i32,
) -> GeneratorDefinition {
    GeneratorDefinition {
        identity: GeneratorIdentity {
            entity: EntityId(1),
            incarnation: 1,
            content_revision: 1,
            random_identity: [1; 16],
        },
        profiles,
        location: GeneratorLocation {
            cell: 0x01010001,
            origin: [0.; 3],
            rotation: [0., 0., 0., 1.],
        },
        kind: GeneratorKind::Object,
        initial_count,
        maximum_count,
        regeneration_interval: 1.,
        initial_delay: 0.,
        regeneration_timestamp: 0.,
        time_type: GeneratorTimeType::Undefined,
        event: None,
        start_time: 0,
        end_time: 0,
        disabled: false,
        automatic_destruction: false,
        parent: None,
        destruction: GeneratorDestruction::Destroy,
        end_destruction: GeneratorDestruction::Destroy,
        rotation_type: GeneratorRotationType::Undefined,
        use_rotation_offset: true,
        radius: 0.,
        vendor_shop_uses_generator: false,
    }
}
#[test]
fn compiled_ace_selection_fixtures() {
    for line in include_str!("../../tests/ace_selection.tsv").lines() {
        let f: Vec<_> = line.split('|').collect();
        let name = f[0];
        let (init, max, powering, unit, profiles) = match name {
            "threshold" | "maxed_threshold" | "unavailable_threshold" => (
                1,
                2,
                false,
                0.5,
                vec![profile(0.99, 1, 1), profile(1., 1, 1)],
            ),
            "reset_threshold" => (
                1,
                3,
                false,
                0.75,
                vec![profile(0.6, 1, 1), profile(0.2, 1, 1), profile(0.8, 1, 1)],
            ),
            "unconditional" => (
                3,
                4,
                true,
                0.999,
                vec![
                    profile(-1., 1, 1),
                    profile(-1., 1, 1),
                    profile(0.5, 1, 1),
                    profile(1., 1, 1),
                ],
            ),
            "unconditional_capped" => (
                1,
                1,
                true,
                0.9,
                vec![profile(-1., 1, 1), profile(-1., 1, 1)],
            ),
            "placeholder_probability" => {
                (1, 1, true, 0.1, vec![profile(0.8, 1, 1), profile(1., 1, 1)])
            }
            "batch_exceeds_initial" => (1, 8, true, 0.5, vec![profile(-1., 5, 8)]),
            "infinite_max" => (3, 9, true, 0.5, vec![profile(-1., 5, -1)]),
            "infinite_initial" => (3, 9, true, 0.5, vec![profile(-1., -1, 5)]),
            "treasure_clamp" => (3, 9, true, 0.5, vec![profile(-1., 4, 8)]),
            "treasure_occupancy" => (3, 9, true, 0.5, vec![profile(-1., 1, 1), profile(1., 1, 1)]),
            "zero_initial" => (1, 1, true, 0.5, vec![profile(-1., 0, 1)]),
            "zero_probability" => (1, 1, true, 0., vec![profile(0., 1, 1)]),
            "partial_capacity" => (1, 3, false, 0.5, vec![profile(-1., 5, 8)]),
            _ => panic!("unknown oracle case {name}"),
        };
        let profiles = profiles
            .into_iter()
            .enumerate()
            .map(|(i, mut p)| {
                p.id = i as u32;
                p
            })
            .collect();
        let mut m = GeneratorMachine::new(
            Arc::new(definition(profiles, init, max)),
            clock(),
            GeneratorLimits::default(),
        )
        .unwrap();
        m.powering = powering;
        match name {
            "maxed_threshold" => {
                m.profiles[0].members.insert(EntityId(2), 1);
            }
            "unavailable_threshold" => m.profiles[0].available_after = Some(1),
            "placeholder_probability" => m.profiles[0].profile.weenie_class_id = 3666,
            "treasure_clamp" => m.profiles[0].profile.where_create = 64,
            "treasure_occupancy" => {
                m.profiles[0].treasure = true;
                m.profiles[0].profile.where_create = 64;
                for id in 2..6 {
                    m.profiles[0].members.insert(EntityId(id), 1);
                }
            }
            "partial_capacity" => {
                for id in 2..4 {
                    m.profiles[0].members.insert(EntityId(id), 1);
                }
            }
            _ => {}
        }
        assert_eq!(
            m.total_probability(1),
            f[1].parse::<f32>().unwrap(),
            "{name}"
        );
        for (i, expected) in f[2].split(',').enumerate() {
            assert_eq!(
                m.adjusted_probability(i, 1),
                Some(expected.parse::<f32>().unwrap()),
                "{name}:{i}"
            );
        }
        let root = RandomRoot::new([7; 32], 1).unwrap();
        if !m.stop(1) {
            m.select_roll(clock(), &root, unit, &mut GeneratorTransition::default())
                .unwrap();
        }
        for (i, expected) in f[3].split(',').enumerate() {
            assert_eq!(
                m.profiles[i].queued,
                expected.parse::<usize>().unwrap(),
                "{name}:{i}"
            );
        }
        for (i, expected) in f[4].split(',').enumerate() {
            assert_eq!(
                m.profiles[i].profile.init_create,
                expected.parse::<i32>().unwrap(),
                "{name}:{i}"
            );
        }
        for (i, expected) in f[5].split(',').enumerate() {
            assert_eq!(
                m.profiles[i].profile.max_create,
                expected.parse::<i32>().unwrap(),
                "{name}:{i}"
            );
        }
    }
}
