#[allow(dead_code, unused_imports)]
#[path = "magic_common/mod.rs"]
mod common;
use bace_gameplay_api::{
    social::SocialIdentity,
    staff::{StaffError, StaffEvent, StaffPrivileges, StaffRegistration},
};
use bace_social::{SocialPreferences, SocialPresence};
use common::*;
fn fixture(capacity: usize) -> Kernel {
    let mut k = kernel(capacity);
    k.register_staff(StaffRegistration {
        binding: CharacterBinding {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(7),
        },
        privileges: StaffPrivileges {
            account_access: 5,
            admin: true,
            ..Default::default()
        },
    })
    .unwrap();
    k.register_social_presence(
        SocialPresence {
            identity: SocialIdentity {
                character: EntityId(1),
                account: AccountId(1),
                name: "+Staff".into(),
            },
            access: 5,
            online: true,
            appear_offline: true,
            afk: false,
            gagged: true,
            olthoi: false,
            no_olthoi_talk: false,
            ignore_fellowship_requests: false,
            auto_accept_fellowship: false,
            share_fellowship_loot: false,
            society: 0,
            listen_allegiance: false,
            listen_general: false,
            listen_trade: false,
            listen_lfg: false,
            listen_roleplay: false,
            listen_society: false,
        },
        SocialPreferences::default(),
    )
    .unwrap();
    k
}
fn decode(s: &str) -> String {
    String::from_utf8(
        s.as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}
#[test]
fn five_staff_broadcast_aliases_match_original_handlers_including_exact_log_text() {
    let mut cases = 0;
    for line in include_str!("fixtures/staff_broadcast.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line.split('|').collect();
        if row[1] == "True" {
            continue;
        } // Host console authentication is a separate adapter.
        let kind: usize = row[0].parse().unwrap();
        let mut k = fixture(8);
        k.staff_broadcast(
            context(1),
            8,
            decode(row[2]),
            kind >= 2,
            kind == 1 || kind == 3,
            false,
        )
        .unwrap();
        let StaffEvent::Broadcast {
            token,
            sender,
            recipients,
            text,
            ..
        } = k.take_staff_event().unwrap()
        else {
            panic!("broadcast missing")
        };
        assert_eq!(token, 8);
        assert_eq!(sender, "+Staff");
        assert_eq!(text, decode(row[3]));
        assert_eq!(text, decode(row[5]));
        assert_eq!(row[4], "20");
        assert_eq!(
            recipients,
            vec![CharacterBinding {
                actor: EntityId(1),
                account: AccountId(1),
                session: SessionId(7)
            }]
        );
        cases += 1;
    }
    assert_eq!(cases, 15);
}
#[test]
fn broadcast_rechecks_alias_privilege_and_retains_sequence_under_pressure() {
    let mut k = fixture(2);
    k.refresh_staff(StaffRegistration {
        binding: CharacterBinding {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(7),
        },
        privileges: StaffPrivileges {
            account_access: 3,
            envoy: true,
            ..Default::default()
        },
    })
    .unwrap();
    assert_eq!(
        k.staff_broadcast(context(1), 8, "test".into(), false, true, false),
        Err(StaffError::NotAuthorized)
    );
    k.staff_broadcast(context(1), 8, "test".into(), false, false, false)
        .unwrap();
    assert_eq!(
        k.staff_broadcast(context(2), 9, "later".into(), false, false, false),
        Err(StaffError::Capacity)
    );
    k.take_staff_event().unwrap();
    k.staff_broadcast(context(2), 9, "later".into(), false, false, false)
        .unwrap();
    k.take_staff_event().unwrap();
    assert_eq!(
        k.staff_broadcast(context(2), 9, "replay".into(), false, false, false),
        Err(StaffError::Stale)
    );
}
