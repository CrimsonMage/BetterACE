use bace_character::ui::apply;
use bace_gameplay_api::{CharacterUi, UiError, UiRequest};
#[test]
fn malformed_options_are_atomic_and_prelogin_defaults_cannot_erase_settings() {
    let mut state = CharacterUi {
        options1: 42,
        gameplay: vec![9, 8],
        ..Default::default()
    };
    let before = state.clone();
    assert_eq!(
        apply(&mut state, UiRequest::Filters(0), false, &[], &[]),
        Err(UiError::BeforeEntry)
    );
    assert_eq!(state, before);
    let bad = UiRequest::Options {
        options1: 0,
        options2: None,
        filters: 0,
        shortcuts: None,
        bars: vec![vec![7]],
        components: None,
        gameplay: None,
    };
    assert_eq!(
        apply(&mut state, bad, true, &[], &[]),
        Err(UiError::UnknownSpell)
    );
    assert_eq!(state, before);
}
#[test]
fn bars_and_component_clear_preserve_other_settings() {
    let mut state = CharacterUi::default();
    apply(
        &mut state,
        UiRequest::AddFavorite {
            spell: 10,
            bar: 7,
            position: 0,
        },
        true,
        &[10],
        &[1],
    )
    .unwrap();
    apply(
        &mut state,
        UiRequest::Component {
            template: 1,
            quantity: 100,
        },
        true,
        &[10],
        &[1],
    )
    .unwrap();
    let before = state.clone();
    assert!(
        apply(
            &mut state,
            UiRequest::Component {
                template: 1,
                quantity: -1
            },
            true,
            &[10],
            &[1]
        )
        .is_err()
    );
    assert_eq!(state, before);
    apply(
        &mut state,
        UiRequest::Component {
            template: 0,
            quantity: -1,
        },
        true,
        &[10],
        &[1],
    )
    .unwrap();
    assert!(state.components.is_empty());
    assert_eq!(state.bars[7], [10]);
}
