use bace_gameplay_api::{
    ActionContext, CharacterBinding, CombatChange, CombatOutcome, CombatRejection as E, SessionId,
};
use bace_replication::{
    BatchLimits, EventSequencer, SequenceKind, Sequences, combat_actions::combat_error_code,
};
use bace_types::{AccountId, EntityId};
#[test]
fn source_codes_and_atomic_shared_counter_output() {
    let codes: std::collections::BTreeMap<_, _> = include_str!("fixtures/cast_errors.csv")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .map(|s| {
            let (k, v) = s.split_once(',').unwrap();
            (k, v.parse::<u32>().unwrap())
        })
        .collect();
    for (error, name) in [
        (E::InvalidRequest, "WERROR_BAD_PARAM"),
        (E::Busy, "WERROR_ACTIONS_LOCKED"),
        (E::MissingActor, "WERROR_OBJECT_GONE"),
        (E::Dead, "WERROR_DEAD"),
        (E::OutOfRange, "WERROR_TOO_FAR"),
        (E::Obstructed, "WERROR_CANT_GET_THERE"),
    ] {
        assert_eq!(combat_error_code(error), codes[name]);
    }
    let binding = CharacterBinding {
        actor: EntityId(3),
        account: AccountId(2),
        session: SessionId(1),
    };
    let mut events = EventSequencer::new(binding, 7);
    let mut properties = Sequences::new(32).unwrap();
    let context = ActionContext {
        actor: binding.actor,
        account: binding.account,
        session: binding.session,
        sequence: 4,
    };
    let limits = BatchLimits {
        max_messages: 1,
        max_bytes: 64,
        max_message_bytes: 64,
        max_string_bytes: 0,
    };
    let mode = CombatOutcome {
        context,
        result: Ok(CombatChange::Mode(2)),
    };
    let mut short = limits;
    short.max_bytes = 0;
    assert!(
        events
            .project_combat_outcome(binding, &mode, &mut properties, short)
            .is_err()
    );
    assert_eq!(
        properties.current(SequenceKind::PropertyInt, 40),
        u8::MAX as u16
    );
    assert_eq!(events.next_sequence(), 7);
    let mode = events
        .project_combat_outcome(binding, &mode, &mut properties, limits)
        .unwrap();
    assert_eq!(mode.messages.len(), 1);
    assert_eq!(&mode.messages[0].bytes[5..], &[40, 0, 0, 0, 2, 0, 0, 0]);
    assert_eq!(
        events.next_sequence(),
        7,
        "a property update is not a game event"
    );
    let fail = CombatOutcome {
        context,
        result: Err(E::Busy),
    };
    let batch = events
        .project_combat_outcome(binding, &fail, &mut properties, limits)
        .unwrap();
    assert_eq!(&batch.messages[0].bytes[16..], &29u32.to_le_bytes());
    assert_eq!(events.next_sequence(), 8);
    let forged = CombatOutcome {
        context: ActionContext {
            session: SessionId(99),
            ..context
        },
        ..fail
    };
    assert!(
        events
            .project_combat_outcome(binding, &forged, &mut properties, limits)
            .is_err()
    );
    assert_eq!(events.next_sequence(), 8);
    for result in [
        CombatChange::AttackStarted {
            target: EntityId(4),
        },
        CombatChange::Cancelled,
    ] {
        assert!(
            events
                .project_combat_outcome(
                    binding,
                    &CombatOutcome {
                        context,
                        result: Ok(result)
                    },
                    &mut properties,
                    limits
                )
                .unwrap()
                .messages
                .is_empty()
        );
    }
}
