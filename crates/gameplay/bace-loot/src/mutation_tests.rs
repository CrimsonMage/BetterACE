use super::*;
use bace_random::{Domain, RandomRoot};
#[test]
fn float_interval_excludes_rounded_upper_endpoint_and_constants_remain_exact() {
    let root = RandomRoot::new([0x37; 32], 1).unwrap();
    let mut stream = root.event_stream([1; 16], Domain::OrdinaryLoot).unwrap();
    let minimum = 1.0f64;
    let maximum = minimum.next_up();
    let range = LootMutationV1::Float {
        property: 1,
        minimum,
        maximum,
    };
    range.validate().unwrap();
    for _ in 0..1000 {
        assert_eq!(sample(&range, &mut stream).unwrap(), M::Float(1, minimum));
    }
    let fixed = LootMutationV1::Float {
        property: 2,
        minimum: f64::MAX,
        maximum: f64::MAX,
    };
    assert_eq!(sample(&fixed, &mut stream).unwrap(), M::Float(2, f64::MAX));
    assert!(
        LootMutationV1::Float {
            property: 1,
            minimum: -f64::MAX,
            maximum: f64::MAX
        }
        .validate()
        .is_err()
    );
}
#[test]
fn full_integer_range_and_extreme_singletons_do_not_overflow() {
    let root = RandomRoot::new([0x41; 32], 1).unwrap();
    let mut stream = root.event_stream([1; 16], Domain::OrdinaryLoot).unwrap();
    for value in [i64::MIN, i64::MAX] {
        assert_eq!(
            sample(
                &LootMutationV1::Int64 {
                    property: 1,
                    minimum: value,
                    maximum: value
                },
                &mut stream
            )
            .unwrap(),
            M::Int64(1, value)
        );
    }
    for _ in 0..100 {
        let value = sample(
            &LootMutationV1::Int64 {
                property: 1,
                minimum: i64::MIN,
                maximum: i64::MAX,
            },
            &mut stream,
        )
        .unwrap();
        assert!(matches!(value, M::Int64(1, _)));
    }
}
