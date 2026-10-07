use bace_replication::{SequenceKind, Sequences};
#[test]
fn property_object_and_motion_wrap_match_unmodified_official_counters() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/primitives.json")).unwrap();
    let sequences = &vectors["vectors"]["sequences"];
    let mut counters = Sequences::new(3).unwrap();
    for expected in sequences["property"].as_array().unwrap() {
        assert_eq!(
            u64::from(counters.advance(SequenceKind::PropertyInt, 1).unwrap()),
            expected.as_u64().unwrap()
        );
    }
    for (kind, total, skip, name) in [
        (
            SequenceKind::ObjectPosition,
            65537,
            65533,
            "object_boundary",
        ),
        (SequenceKind::Motion, 32769, 32765, "motion_boundary"),
    ] {
        let actual: Vec<u64> = (0..total)
            .map(|_| u64::from(counters.advance(kind, 0).unwrap()))
            .skip(skip)
            .collect();
        let expected: Vec<_> = sequences[name]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap())
            .collect();
        assert_eq!(actual, expected);
    }
}
