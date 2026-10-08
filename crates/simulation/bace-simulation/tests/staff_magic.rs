#[allow(dead_code, unused_imports)]
#[path = "magic_common/mod.rs"]
mod common;
use bace_gameplay_api::staff::*;
use bace_magic::{EnchantmentMetadata, EnchantmentSpec};
use common::*;
fn setup(capacity: usize) -> Kernel {
    let mut k = kernel(capacity);
    let binding = CharacterBinding {
        session: SessionId(7),
        account: AccountId(1),
        actor: EntityId(1),
    };
    k.register_staff(StaffRegistration {
        binding,
        privileges: StaffPrivileges {
            account_access: 5,
            admin: true,
            ..Default::default()
        },
    })
    .unwrap();
    k.register_character_ui(
        binding,
        bace_simulation::OwnedUiState {
            state: Default::default(),
            known_spells: vec![100],
            component_templates: vec![],
            entered: true,
        },
    )
    .unwrap();
    k.register_staff_spell_definitions(vec![StaffSpellDefinition {
        spell: 1644,
        name: "Sentinel's Run".into(),
        enum_name: "SentinelRun".into(),
        targeted: true,
    }])
    .unwrap();
    k
}
fn run_spell() -> PreparedMagicSpell {
    spell(
        1644,
        SpellEffect::Enchantment(EnchantmentSpec {
            category: 123,
            power: 100,
            duration: 30.,
            layer: 0,
            stat_type: 1,
            stat_key: 1,
            value: 10.,
            beneficial: true,
            set_id: None,
        }),
    )
}
#[test]
fn spellbook_is_unchanged_until_exact_receipt_and_rejection_releases() {
    let mut k = setup(16);
    let ticket = k
        .prepare_staff_spellbook(context(1), 5, 1644, true, false)
        .unwrap();
    assert_eq!(
        k.character(EntityId(1)).unwrap().revision(),
        ticket.before_revision
    );
    assert!(matches!(
        k.take_staff_event(),
        Some(StaffEvent::SpellProposal(_))
    ));
    let mut wrong = ticket.clone();
    wrong.operation = 6;
    assert_eq!(k.confirm_staff_spellbook(&wrong), Err(StaffError::Stale));
    k.reject_staff_spellbook(&ticket).unwrap();
    let retry = k
        .prepare_staff_spellbook(context(2), 7, 1644, true, false)
        .unwrap();
    assert_eq!(retry.before, vec![100]);
    k.take_staff_event();
    k.confirm_staff_spellbook(&retry).unwrap();
    assert_eq!(
        k.character(EntityId(1)).unwrap().revision(),
        retry.after_revision
    );
    assert!(matches!(
        k.take_staff_event(),
        Some(StaffEvent::Spellbook {
            learn: true,
            changed: true,
            ..
        })
    ));
    let next = k
        .prepare_staff_spellbook(context(3), 8, 1644, true, false)
        .unwrap();
    assert_eq!(next.before, next.after);
    k.take_staff_event();
    k.reject_staff_spellbook(&next).unwrap();
}
#[test]
fn run_and_buff_fail_atomically_on_missing_assets_then_use_timed_registry() {
    let mut k = setup(128);
    k.register_magic_spell(run_spell()).unwrap();
    k.register_enchantment_metadata(1644, EnchantmentMetadata::default())
        .unwrap();
    k.staff_run(context(1), StaffRunMode::On, false).unwrap();
    assert_eq!(
        k.magic_registry(EntityId(1)).unwrap().entries()[0].spell,
        1644
    );
    k.take_staff_event();
    k.staff_run(context(2), StaffRunMode::Off, false).unwrap();
    k.take_staff_event();
    assert!(k.magic_registry(EntityId(1)).unwrap().entries().is_empty());
    k.register_staff_buff_plans(vec![StaffBuffPlan {
        level: 8,
        self_spells: vec![1644, 999],
        other_spells: vec![1644],
        banes: vec![],
        effects: vec![(1644, 1)],
        missing: vec![],
    }])
    .unwrap();
    assert!(
        k.staff_buff(context(3), EntityId(1), false, 8, vec![], false)
            .is_err()
    );
    assert!(k.magic_registry(EntityId(1)).unwrap().entries().is_empty());
}
#[test]
fn admin_xp_waits_for_durable_receipt_before_granted_text_and_audit() {
    let mut k = setup(32);
    let actor = EntityId(1);
    k.register_npc_character_services(
        actor,
        bace_character::CharacterServiceState {
            level: 1,
            total_experience: 0,
            total_skill_credits: None,
            titles: vec![],
            enlightenment: 0,
            sanctuary: None,
        },
        bace_quests::ContractRegistry::restore(vec![]).unwrap(),
    )
    .unwrap();
    k.configure_social_experience(Arc::new(
        bace_character::CharacterLevelTable::prepare(vec![0, 0, 100, 300], vec![0, 0, 0, 0])
            .unwrap(),
    ))
    .unwrap();
    k.register_inventory_container(bace_inventory::InventoryContainer {
        id: actor,
        revision: 0,
        root_owner: Some(actor),
        slots: 8,
        pack_slots: 2,
        burden_limit: 1000,
        accessible: true,
        open: false,
        generation: 0,
    })
    .unwrap();
    let presence = bace_social::SocialPresence {
        identity: bace_gameplay_api::social::SocialIdentity {
            character: actor,
            account: AccountId(1),
            name: "Rune".into(),
        },
        access: 5,
        online: true,
        appear_offline: false,
        afk: false,
        gagged: true,
        olthoi: false,
        no_olthoi_talk: false,
        ignore_fellowship_requests: false,
        auto_accept_fellowship: false,
        share_fellowship_loot: false,
        society: 0,
        listen_allegiance: true,
        listen_general: true,
        listen_trade: true,
        listen_lfg: true,
        listen_roleplay: true,
        listen_society: true,
    };
    k.register_social_presence(
        presence,
        bace_social::SocialPreferences {
            channels: BTreeSet::from([4]),
            ..Default::default()
        },
    )
    .unwrap();
    while k.take_social_event().is_some() {}
    let before = k.character(actor).unwrap().available_experience();
    k.staff_grant_experience(context(1), actor, 50, 123, false)
        .unwrap();
    assert_eq!(k.character(actor).unwrap().available_experience(), before);
    assert!(k.take_staff_event().is_none());
    assert!(k.take_social_event().is_none());
    let ticket = k.take_allegiance_proposal().unwrap();
    k.confirm_allegiance_committed(&ticket).unwrap();
    assert_eq!(
        k.character(actor).unwrap().available_experience(),
        before + 50
    );
    assert!(matches!(
        k.take_staff_event(),
        Some(StaffEvent::Outcome {
            token: 123,
            result: Ok(()),
            ..
        })
    ));
    let events: Vec<_> = std::iter::from_fn(|| k.take_social_event()).collect();
    assert!(events.iter().any(|e|matches!(e,bace_gameplay_api::social::SocialEvent::System{text,chat_type:13,..} if text=="50 experience granted.")));
    assert!(events.iter().any(|e|matches!(e,bace_gameplay_api::social::SocialEvent::Chat{accepted,recipients,..} if accepted.channel==bace_gameplay_api::social::ChatChannel::Audit&&accepted.text=="Rune granted 50 experience to Rune."&&recipients==&vec![actor])));
    assert!(k.confirm_allegiance_committed(&ticket).is_err());
}

#[test]
fn staff_direct_effect_catalog_is_atomic_and_never_claims_player_casting_ids() {
    let mut k = kernel(32);
    k.register_staff(StaffRegistration {
        binding: CharacterBinding {
            session: SessionId(7),
            account: AccountId(1),
            actor: EntityId(1),
        },
        privileges: StaffPrivileges {
            account_access: 5,
            admin: true,
            ..Default::default()
        },
    })
    .unwrap();
    let definitions = vec![StaffSpellDefinition {
        spell: 1644,
        name: "Sentinel's Run".into(),
        enum_name: "SentinelRun".into(),
        targeted: true,
    }];
    let prepared = run_spell();
    let SpellEffect::Enchantment(spec) = prepared.spell.effect.clone() else {
        panic!("test enchantment");
    };
    let entry = bace_magic::EnchantmentEntry {
        spell: 1644,
        caster: 0,
        school: prepared.spell.school,
        spec,
        start_time: 0.,
        is_set_spell: true,
        is_level8_aura: true,
        metadata: EnchantmentMetadata::default(),
    };
    let mut invalid = entry.clone();
    invalid.spell = 999;
    assert_eq!(
        k.register_staff_magic_assets(definitions.clone(), vec![], vec![entry.clone(), invalid]),
        Err(StaffError::Invalid)
    );
    // Failed admission must not leave any definitions or effects installed.
    k.register_staff_magic_assets(definitions, vec![], vec![entry])
        .unwrap();
    k.staff_run(context(1), StaffRunMode::On, false).unwrap();
    let applied = &k.magic_registry(EntityId(1)).unwrap().entries()[0];
    assert!(applied.is_set_spell && applied.is_level8_aura);
    assert_eq!(applied.caster, 1);
    // Staff installation must leave the full native casting slot unoccupied.
    k.register_magic_spell(prepared.clone()).unwrap();
    assert!(k.register_magic_spell(prepared).is_err());
}
