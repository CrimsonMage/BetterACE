use bace_gameplay_api::{CastRejection as E, CharacterBinding, SessionId};
use bace_replication::{BatchLimits, EventSequencer, cast_output::cast_error_code};
use bace_types::{AccountId, EntityId};
#[test]
fn cast_policy_codes_match_original_gdle_enum_and_rejected_batch_preserves_counter() {
    let rows: std::collections::BTreeMap<_, _> = include_str!("fixtures/cast_errors.csv")
        .lines()
        .filter(|r| !r.starts_with('#'))
        .map(|r| {
            let (k, v) = r.split_once(',').unwrap();
            (k, v.parse::<u32>().unwrap())
        })
        .collect();
    for (error, key) in [
        (E::Busy, "WERROR_ACTIONS_LOCKED"),
        (E::UnlearnedSpell, "WERROR_MAGIC_UNLEARNED_SPELL"),
        (E::UntrainedSchool, "WERROR_MAGIC_INVALID_SPELL_TYPE"),
        (E::MissingComponents, "WERROR_MAGIC_MISSING_COMPONENTS"),
        (E::InsufficientMana, "WERROR_MAGIC_INSUFFICIENT_MANA"),
        (E::Fizzled, "WERROR_MAGIC_FIZZLE"),
        (E::InvalidTarget, "WERROR_MAGIC_BAD_TARGET_TYPE"),
        (E::OutOfRange, "WERROR_MISSILE_OUT_OF_RANGE"),
        (E::MissingAssets, "WERROR_MAGIC_GENERAL_FAILURE"),
    ] {
        assert_eq!(cast_error_code(error), rows[key]);
    }
    let binding = CharacterBinding {
        session: SessionId(1),
        account: AccountId(2),
        actor: EntityId(3),
    };
    let mut seq = EventSequencer::new(binding, u32::MAX);
    let limits = BatchLimits {
        max_messages: 4,
        max_bytes: 1024,
        max_message_bytes: 1024,
        max_string_bytes: 128,
    };
    let mut too_small = limits;
    too_small.max_messages = 0;
    assert!(seq.project_cast_done(binding, 0, too_small).is_err());
    assert_eq!(seq.next_sequence(), u32::MAX);
    let batch = seq.project_cast_done(binding, 1025, limits).unwrap();
    assert_eq!(batch.messages.len(), 1);
    assert_eq!(seq.next_sequence(), 0);
    assert_eq!(&batch.messages[0].bytes[16..], &1025u32.to_le_bytes());
}
