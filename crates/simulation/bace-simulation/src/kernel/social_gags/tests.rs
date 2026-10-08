use super::*;
use bace_character::{CharacterProgression, ProgressionTables, RankTable};
use bace_gameplay_api::staff::{StaffPrivileges, StaffRegistration};
use bace_social::{SocialPreferences, SocialPresence};
use std::sync::Arc;
fn fixture() -> Kernel {
    let mut k = crate::synthetic_scenario(1, 0).unwrap();
    let rank = RankTable::new(&[0, 10]).unwrap();
    k.register_character(
        binding(),
        CharacterProgression::new(
            &[],
            Arc::new(ProgressionTables {
                attributes: rank.clone(),
                vitals: rank.clone(),
                trained_skills: rank.clone(),
                specialized_skills: rank,
            }),
            0,
            0,
        )
        .unwrap(),
    )
    .unwrap();
    k.register_magic_registry(
        EntityId(1),
        bace_magic::EnchantmentRegistry::new(16).unwrap(),
        true,
    )
    .unwrap();
    k.register_staff(StaffRegistration {
        binding: binding(),
        privileges: StaffPrivileges {
            account_access: 5,
            admin: true,
            ..Default::default()
        },
    })
    .unwrap();
    k.register_social_presence(
        SocialPresence {
            identity: identity(),
            access: 5,
            online: true,
            appear_offline: false,
            afk: false,
            gagged: false,
            olthoi: false,
            no_olthoi_talk: false,
            ignore_fellowship_requests: false,
            auto_accept_fellowship: false,
            share_fellowship_loot: false,
            society: 0,
            listen_allegiance: false,
            listen_general: true,
            listen_trade: true,
            listen_lfg: false,
            listen_roleplay: false,
            listen_society: false,
        },
        SocialPreferences::default(),
    )
    .unwrap();
    k.register_gag(EntityId(1), GagRecovery::default()).unwrap();
    k.social.events.clear();
    k
}
fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(7),
    }
}
fn identity() -> SocialIdentity {
    SocialIdentity {
        character: EntityId(1),
        account: bace_types::AccountId(1),
        name: "Staff".into(),
    }
}
fn context(sequence: u32) -> ActionContext {
    let b = binding();
    ActionContext {
        actor: b.actor,
        account: b.account,
        session: b.session,
        sequence,
    }
}
fn propose(k: &mut Kernel, enabled: bool, sequence: u32) -> StaffGagProposal {
    k.prepare_staff_gag(
        context(sequence),
        u64::from(sequence),
        identity(),
        "Staff".into(),
        enabled,
        100.,
        false,
    )
    .unwrap();
    let StaffEvent::GagProposal(p) = k.take_staff_event().unwrap() else {
        panic!("proposal")
    };
    p
}
#[test]
fn gag_waits_for_exact_receipt_and_retains_owner_on_output_pressure() {
    let mut k = fixture();
    let revision = k.characters.get(EntityId(1)).unwrap().revision();
    let p = propose(&mut k, true, 1);
    assert!(!k.gag_snapshot(EntityId(1)).unwrap().state.active);
    assert!(k.gag_pending(EntityId(1)));
    let mut wrong = p.clone();
    wrong.unix_seconds += 1.;
    assert_eq!(k.complete_staff_gag(&wrong, true), Err(E::Stale));
    k.social.capacity = 0;
    assert_eq!(k.complete_staff_gag(&p, true), Err(E::Capacity));
    assert!(k.characters.reserved(EntityId(1)));
    assert_eq!(k.characters.get(EntityId(1)).unwrap().revision(), revision);
    k.social.capacity = 8;
    k.complete_staff_gag(&p, true).unwrap();
    assert_eq!(
        k.characters.get(EntityId(1)).unwrap().revision(),
        revision + 1
    );
    assert!(k.social.directory.presence(EntityId(1)).unwrap().gagged);
    assert!(
        matches!(&k.social.events[0],SocialEvent::Chat{accepted,..} if accepted.text=="Staff has gagged Staff for five minutes.")
    );
    assert!(
        matches!(&k.social.events[1],SocialEvent::System{text,chat_type:20,..} if text=="Staff has been gagged for five minutes.")
    );
    assert_eq!(k.complete_staff_gag(&p, true), Err(E::Stale));
}
#[test]
fn gag_heartbeat_notice_and_expiry_hold_exact_dirty_state_until_output_room() {
    let mut k = fixture();
    let p = propose(&mut k, true, 1);
    k.complete_staff_gag(&p, true).unwrap();
    k.social.events.clear();
    k.queue_gag_heartbeat(EntityId(1), 5.);
    k.social.capacity = 0;
    k.step_social_gags().unwrap();
    assert_eq!(k.gag_snapshot(EntityId(1)).unwrap().state.remaining, 300.);
    k.social.capacity = 8;
    k.step_social_gags().unwrap();
    assert_eq!(k.gag_snapshot(EntityId(1)).unwrap().state.remaining, 295.);
    assert!(
        matches!(&k.social.events[0],SocialEvent::Transient{text,..} if text=="Your chat privileges have been suspended.")
    );
    let recovered = k.gag_snapshot(EntityId(1)).unwrap();
    assert!(recovered.state.noticed);
    k.social_gags.states.remove(&EntityId(1));
    k.tick = 90000;
    k.register_gag(EntityId(1), recovered).unwrap();
    assert_eq!(k.gag_snapshot(EntityId(1)), Some(recovered));
    k.social.events.clear();
    k.queue_gag_heartbeat(EntityId(1), 295.);
    k.step_social_gags().unwrap();
    assert!(!k.social.directory.presence(EntityId(1)).unwrap().gagged);
    assert!(
        matches!(&k.social.events[0],SocialEvent::Transient{text,..} if text=="Your chat privileges have been restored.")
    );
}
#[test]
fn rejected_gag_releases_only_its_hold_without_gag_mutation() {
    let mut k = fixture();
    let old = k.gag_snapshot(EntityId(1));
    let p = propose(&mut k, true, 1);
    k.complete_staff_gag(&p, false).unwrap();
    assert_eq!(k.gag_snapshot(EntityId(1)), old);
    assert!(!k.characters.reserved(EntityId(1)));
    assert!(
        matches!(&k.social.events[0],SocialEvent::System{text,..} if text=="Unable to gag a character named Staff, check the name and re-try the command.")
    );
}
fn unhex(s: &str) -> String {
    String::from_utf8(
        s.as_bytes()
            .chunks_exact(2)
            .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}
#[test]
fn accepted_gag_and_ungag_match_original_manager_and_handler_vectors() {
    let mut count = 0;
    for row in include_str!("../../../tests/fixtures/staff_gag.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let v: Vec<_> = row.split('|').collect();
        let enabled = v[0] == "True";
        let mut k = fixture();
        let p = propose(&mut k, enabled, 1);
        k.complete_staff_gag(&p, v[1] == "True").unwrap();
        if v[1] != "True" {
            assert!(
                matches!(&k.social.events[0],SocialEvent::System{text,..} if text==&unhex(v[7]))
            );
            count += 1;
            continue;
        }
        let state = k.gag_snapshot(EntityId(1)).unwrap().state;
        assert_eq!(state.active, v[2] == "True");
        assert_eq!(state.remaining, v[4].parse::<f64>().unwrap());
        assert!(
            matches!(&k.social.events[0],SocialEvent::Chat{accepted,..} if accepted.text==unhex(v[6]))
        );
        assert!(
            matches!(&k.social.events[1],SocialEvent::System{text,chat_type:20,..} if text==&unhex(v[7]))
        );
        count += 1;
    }
    assert_eq!(count, 4);
}
#[test]
fn prior_heartbeat_cannot_debit_new_gag_and_pressure_keeps_command_owned() {
    let mut k = fixture();
    let p = propose(&mut k, true, 1);
    k.complete_staff_gag(&p, true).unwrap();
    k.social.events.clear();
    k.queue_gag_heartbeat(EntityId(1), 5.);
    k.social.capacity = 0;
    let command = bace_gameplay_api::staff::StaffCommand {
        token: 2,
        action: bace_gameplay_api::staff::StaffAction::Gag {
            context: context(2),
            target: identity(),
            requested_name: "Staff".into(),
            enabled: true,
            unix_seconds: 200.,
            sudo: false,
        },
    };
    let retained = k.apply_staff_command(command).unwrap_err();
    assert_eq!(k.gag_snapshot(EntityId(1)).unwrap().state.remaining, 300.);
    k.social.capacity = 8;
    k.step_social_gags().unwrap();
    assert_eq!(k.gag_snapshot(EntityId(1)).unwrap().state.remaining, 295.);
    k.social.events.clear();
    k.apply_staff_command(*retained).unwrap();
    let StaffEvent::GagProposal(p) = k.take_staff_event().unwrap() else {
        panic!("proposal")
    };
    k.complete_staff_gag(&p, true).unwrap();
    k.step_social_gags().unwrap();
    assert_eq!(k.gag_snapshot(EntityId(1)).unwrap().state.remaining, 300.);
}
