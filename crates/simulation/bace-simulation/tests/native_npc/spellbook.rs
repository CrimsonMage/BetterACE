use super::*;
#[test]
fn teaching_requires_valid_definition_exact_receipt_and_updates_canonical_spellbook() {
    let mut k = xp_kernel();
    k.register_character_ui(
        binding(),
        bace_simulation::OwnedUiState {
            state: Default::default(),
            known_spells: vec![7],
            component_templates: vec![],
            entered: true,
        },
    )
    .unwrap();
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![EmoteAction {
                r#type: 27,
                spell_id: Some(100),
                ..Default::default()
            }],
        )],
    );
    k.step().unwrap();
    let proposal = k.take_npc_proposal().unwrap();
    assert!(matches!(
        k.prepare_npc_spellbook(&proposal),
        Err(NpcFailure::MissingContent)
    ));
    k.register_npc_spell_catalog(vec![7, 100]).unwrap();
    let ticket = k.prepare_npc_spellbook(&proposal).unwrap();
    assert_eq!(ticket.before, vec![7]);
    assert_eq!(ticket.after, vec![7, 100]);
    assert!(k.synchronize_npc_known_spell(EntityId(1), 100).is_err());
    assert!(
        k.complete_npc_service(
            &proposal,
            bace_gameplay_api::NpcCompletion::Applied { post_delay: 0.0 }
        )
        .is_err()
    );
    let mut wrong = ticket.clone();
    wrong.after.push(101);
    assert!(k.confirm_npc_spellbook_committed(&wrong).is_err());
    assert_eq!(
        k.character(EntityId(1)).unwrap().revision(),
        ticket.before_revision
    );
    k.reject_npc_spellbook(&ticket).unwrap();
    assert!(k.synchronize_npc_known_spell(EntityId(1), 100).is_err());
    let retry = k.prepare_npc_spellbook(&proposal).unwrap();
    assert_eq!(ticket, retry);
    k.confirm_npc_spellbook_committed(&retry).unwrap();
    k.synchronize_npc_known_spell(EntityId(1), 100).unwrap();
    assert_eq!(
        k.character(EntityId(1)).unwrap().revision(),
        ticket.after_revision
    );
    assert!(k.confirm_npc_spellbook_committed(&ticket).is_err());
}
