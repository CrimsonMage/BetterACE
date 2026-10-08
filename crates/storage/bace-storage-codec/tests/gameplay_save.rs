use bace_content::{Property, WeenieV1};
use bace_storage_codec::{CodecError, EntitySaveV1, PlayerSaveV1, QuestSaveV1, SaveCodecError};

fn player() -> PlayerSaveV1 {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "player".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.int64s.push(Property {
        id: 0xffff,
        value: i64::MIN,
    });
    PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x5000_0001,
            template_revision: 1,
            mutation_revision: 5,
            state,
        },
        account_id: 1,
        name: "Test Player".into(),
        metadata: Default::default(),
        quests: vec![QuestSaveV1 {
            name: "quest".into(),
            completions: 2,
            last_completed: 100,
        }],
    }
}

#[test]
fn save_integrity_schema_and_domain_limits_are_enforced() {
    let expected = player();
    let bytes = expected.encode().unwrap();
    assert_eq!(PlayerSaveV1::decode(&bytes).unwrap(), expected);
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(matches!(
        PlayerSaveV1::decode(&corrupt),
        Err(SaveCodecError::Envelope(CodecError::Integrity))
    ));
    let future = bace_storage_codec::encode(100, 2, &expected, Default::default()).unwrap();
    assert!(matches!(
        PlayerSaveV1::decode(&future),
        Err(SaveCodecError::Envelope(CodecError::Schema { .. }))
    ));
    let mut duplicate = expected.clone();
    duplicate.quests.push(duplicate.quests[0].clone());
    assert!(duplicate.encode().is_err());
    // Bypass the domain encoder: bounded decode still rejects hostile collection counts.
    duplicate.quests = vec![expected.quests[0].clone(); 4097];
    let oversized = bace_storage_codec::encode(100, 1, &duplicate, Default::default()).unwrap();
    assert!(PlayerSaveV1::decode(&oversized).is_err());
    let mut nonfinite = expected;
    nonfinite.entity.state.properties.floats.push(Property {
        id: 1,
        value: f64::NAN,
    });
    assert!(nonfinite.encode().is_err());
}
