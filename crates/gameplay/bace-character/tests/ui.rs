use bace_character::ui::apply;
use bace_gameplay_api::{CharacterUi, UiError, UiRequest, UiShortcut};
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

#[test]
fn ace_shortcut_actions_survive_stale_options_shortcut_section() {
    // ACE Player_Character.HandleActionAddShortcut/RemoveShortcut changes the
    // persistent shortcut list. GameActionSetCharacterOptions only consumes its
    // Shortcut section; its handler does not install that client copy.
    let mut state = CharacterUi::default();
    let first = UiShortcut {
        index: 2,
        object: 0x8002_40f0,
        spell: 0,
        layer: 0,
    };
    assert!(
        apply(
            &mut state,
            UiRequest::AddShortcut(first.clone()),
            true,
            &[],
            &[]
        )
        .unwrap()
    );
    assert_eq!(state.shortcuts, std::slice::from_ref(&first));
    assert!(
        apply(
            &mut state,
            UiRequest::Options {
                options1: 1,
                options2: None,
                filters: 0x3fff,
                shortcuts: Some(vec![]),
                bars: vec![],
                components: None,
                gameplay: None,
            },
            true,
            &[],
            &[]
        )
        .unwrap()
    );
    assert_eq!(state.shortcuts, std::slice::from_ref(&first));
    let replacement = UiShortcut {
        object: 0x8002_40f1,
        ..first
    };
    assert!(
        apply(
            &mut state,
            UiRequest::AddShortcut(replacement.clone()),
            true,
            &[],
            &[]
        )
        .unwrap()
    );
    assert_eq!(state.shortcuts, [replacement]);
    assert!(apply(&mut state, UiRequest::RemoveShortcut(2), true, &[], &[]).unwrap());
    assert!(state.shortcuts.is_empty());
}
