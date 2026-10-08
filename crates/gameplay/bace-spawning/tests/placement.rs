mod common;
use bace_gameplay_api::*;
use bace_spawning::*;
use common::*;
#[test]
fn compiled_ace_specific_position_fixtures() {
    for line in include_str!("ace_placement.tsv").lines() {
        let fields: Vec<_> = line.split('|').collect();
        let name = fields[0];
        let mut d = definition();
        d.rotation_type = if name == "relative" {
            GeneratorRotationType::Relative
        } else {
            GeneratorRotationType::Absolute
        };
        d.location.rotation = [
            0.,
            0.,
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        ];
        d.use_rotation_offset = name != "offset_disabled" && name != "missing_offset";
        let p = &mut d.profiles[0];
        p.where_create = 4;
        p.position.cell = if name == "absolute" {
            Some(0x01010002)
        } else {
            None
        };
        p.position.origin = [
            Some(1.),
            if name == "missing_offset" {
                None
            } else {
                Some(2.)
            },
            Some(3.),
        ];
        p.position.rotation = [Some(0.), Some(0.), Some(0.), Some(1.)];
        let GeneratorDestination::Specific(actual) = generator_destination(&d, &d.profiles[0])
        else {
            panic!()
        };
        assert_eq!(actual.cell, fields[1].parse::<u32>().unwrap());
        for (a, e) in actual.origin.iter().zip(fields[2].split(',')) {
            assert!((a - e.parse::<f32>().unwrap()).abs() < 0.00001, "{name}");
        }
        for (a, e) in actual.rotation.iter().zip(fields[3].split(',')) {
            assert!((a - e.parse::<f32>().unwrap()).abs() < 0.00001, "{name}");
        }
    }
}
#[test]
fn mixed_flags_follow_source_precedence_and_wield_defaults() {
    let mut d = definition();
    for (flags, expected) in [
        (4 | 2 | 8 | 32, 4),
        (2 | 8 | 32, 2),
        (8 | 32, 8),
        (32, 32),
        (16 | 64, 0),
        (1, 0),
    ] {
        d.profiles[0].where_create = flags;
        let value = generator_destination(&d, &d.profiles[0]);
        assert!(matches!(
            (expected, value),
            (4, GeneratorDestination::Specific(_))
                | (2, GeneratorDestination::Scatter { .. })
                | (8, GeneratorDestination::Contain { .. })
                | (32, GeneratorDestination::Shop { .. })
                | (0, GeneratorDestination::Default(_))
        ));
    }
}
#[test]
fn scatter_offsets_are_unrotated_and_absolute_profile_is_ignored() {
    let mut d = definition();
    d.profiles[0].where_create = 2;
    d.profiles[0].position.origin = [Some(1.), Some(2.), Some(3.)];
    d.location.rotation = [0., 0., 1., 0.];
    for cell in [None, Some(0x01010002)] {
        d.profiles[0].position.cell = cell;
        let GeneratorDestination::Scatter {
            center, attempts, ..
        } = generator_destination(&d, &d.profiles[0])
        else {
            panic!()
        };
        assert_eq!(
            center.origin,
            if cell.is_none() {
                [11., 22., 33.05]
            } else {
                [10., 20., 30.05]
            }
        );
        assert_eq!(attempts, 20);
    }
}
#[test]
fn linked_profiles_clone_only_source_fields_and_increment_unconditional_counts() {
    let mut d = definition();
    d.profiles[0].palette_id = Some(99);
    d.profiles[0].shade = Some(0.5);
    d.profiles[0].stack_size = Some(10);
    let location = GeneratorLocation {
        cell: 0x01010002,
        origin: [1., 2., 3.],
        rotation: [0., 0., 0., 1.],
    };
    append_generator_links(
        &mut d,
        &[GeneratorLink {
            profile_id: 400,
            weenie_class_id: 200,
            location,
        }],
    )
    .unwrap();
    let p = &d.profiles[1];
    assert_eq!(p.id, 400);
    assert_eq!(p.weenie_class_id, 200);
    assert_eq!(p.palette_id, None);
    assert_eq!(p.shade, None);
    assert_eq!(p.stack_size, None);
    assert_eq!(p.position.origin, [Some(1.), Some(2.), Some(3.)]);
    assert_eq!(d.initial_count, 2);
    assert_eq!(d.maximum_count, 2);
    GeneratorMachine::new(
        std::sync::Arc::new(d.clone()),
        clock(0),
        GeneratorLimits::default(),
    )
    .unwrap();
    let before = d.clone();
    assert_eq!(
        append_generator_links(
            &mut d,
            &[GeneratorLink {
                profile_id: 400,
                weenie_class_id: 201,
                location
            }]
        ),
        Err(GeneratorError::Definition)
    );
    assert_eq!(d, before);
}
