use bace_gameplay_api::social::{ChatChannel, SocialError, SocialIdentity, SocialSquelch};
use bace_social::{SocialDirectory, SocialPreferences, SocialPresence, adjust_turbine_channel};
use bace_types::{AccountId, EntityId};
fn presence(id: u32) -> SocialPresence {
    SocialPresence {
        identity: SocialIdentity {
            character: EntityId(id),
            account: AccountId(id as u64),
            name: format!("Player{id}"),
        },
        access: 0,
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
        listen_allegiance: true,
        listen_general: true,
        listen_trade: true,
        listen_lfg: true,
        listen_roleplay: true,
        listen_society: true,
    }
}
#[test]
fn official_contains_and_channel_adjustment_vectors() {
    let mut count = 0;
    for line in include_str!("fixtures/chat_policy.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<_> = line.split(',').collect();
        let v: Vec<u32> = p[1..].iter().map(|v| v.parse().unwrap()).collect();
        if p[0] == "T" {
            assert_eq!(
                adjust_turbine_channel(v[0], v[1], v[2]).unwrap(),
                (v[3], v[4]),
                "{line}"
            );
        } else {
            let mut directory = SocialDirectory::new(4).unwrap();
            directory
                .register(presence(1), SocialPreferences::default())
                .unwrap();
            let mut settings = SocialPreferences::default();
            if v[0] == 0 {
                settings.global_mask = v[1];
            } else {
                settings.squelches.push(SocialSquelch {
                    character: EntityId(1),
                    account: (v[0] == 2).then_some(AccountId(1)),
                    name: "Player1".into(),
                    mask: v[1],
                });
            }
            directory.register(presence(2), settings).unwrap();
            assert_eq!(
                directory.squelched(EntityId(2), EntityId(1), v[2]),
                v[3] == 1,
                "{line}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 906);
}
#[test]
fn source_filters_allchannels_distinct_from_allegiance_and_global_applies_to_self() {
    let mut directory = SocialDirectory::new(4).unwrap();
    directory
        .register(
            presence(1),
            SocialPreferences {
                global_mask: 4,
                ..Default::default()
            },
        )
        .unwrap();
    directory
        .register(
            presence(2),
            SocialPreferences {
                global_mask: 1 << 18,
                ..Default::default()
            },
        )
        .unwrap();
    let mut admin = presence(3);
    admin.access = 5;
    directory
        .register(admin, SocialPreferences::default())
        .unwrap();
    let mut olthoi = presence(4);
    olthoi.olthoi = true;
    directory
        .register(olthoi, SocialPreferences::default())
        .unwrap();
    assert!(directory.squelched(EntityId(1), EntityId(1), 2));
    let recipients = directory
        .route(
            EntityId(1),
            ChatChannel::General,
            "hello".into(),
            &[EntityId(1), EntityId(2), EntityId(4)],
            1,
            0,
        )
        .unwrap()
        .recipients;
    assert_eq!(recipients, vec![EntityId(1), EntityId(2)]);
    assert_eq!(
        directory.route(EntityId(4), ChatChannel::General, "hello".into(), &[], 2, 0),
        Err(SocialError::Forbidden)
    );
    let mut sender = directory.presence(EntityId(1)).unwrap().clone();
    sender.society = 7;
    directory.update_presence(sender).unwrap();
    assert_eq!(
        directory
            .route(
                EntityId(1),
                ChatChannel::Society(7),
                "hello".into(),
                &[EntityId(1), EntityId(2), EntityId(3)],
                2,
                0
            )
            .unwrap()
            .recipients,
        vec![EntityId(1), EntityId(3)]
    );
}

#[test]
fn original_fellowship_branch_repeats_self_echo_for_each_squelched_member() {
    let mut rows = 0;
    for line in include_str!("fixtures/legacy_chat.csv")
        .lines()
        .filter(|s| s.starts_with("F,"))
    {
        let p: Vec<_> = line.split(',').collect();
        let mask: u32 = p[1].parse().unwrap();
        let online: u32 = p[2].parse().unwrap();
        let mut directory = SocialDirectory::new(4).unwrap();
        let mut candidates = vec![];
        for id in 1..=3 {
            if online & (1 << (id - 1)) == 0 {
                continue;
            }
            let settings = SocialPreferences {
                global_mask: if mask & (1 << (id - 1)) != 0 {
                    1 << 19
                } else {
                    0
                },
                ..Default::default()
            };
            directory.register(presence(id), settings).unwrap();
            candidates.push(EntityId(id));
        }
        let delivery = directory
            .route(
                EntityId(1),
                ChatChannel::Fellowship,
                "hello".into(),
                &candidates,
                1,
                0,
            )
            .unwrap();
        let expected: Vec<_> = p[3]
            .split(';')
            .map(|v| EntityId(v.split(':').next().unwrap().parse().unwrap()))
            .collect();
        assert_eq!(delivery.recipients, expected, "{line}");
        rows += 1;
    }
    assert_eq!(rows, 32);
}

#[test]
fn adjusted_public_type_retains_original_listener_channel() {
    let mut directory = SocialDirectory::new(4).unwrap();
    directory
        .register(presence(1), SocialPreferences::default())
        .unwrap();
    let mut recipient = presence(2);
    recipient.listen_general = false;
    recipient.listen_trade = true;
    directory
        .register(recipient, SocialPreferences::default())
        .unwrap();
    assert!(
        directory
            .route(
                EntityId(1),
                ChatChannel::General,
                "x".into(),
                &[EntityId(2)],
                1,
                0
            )
            .unwrap()
            .recipients
            .is_empty()
    );
    assert_eq!(
        directory
            .route_with_public_listener(
                EntityId(1),
                ChatChannel::General,
                "x".into(),
                &[EntityId(2)],
                (1, 0),
                3
            )
            .unwrap()
            .recipients,
        vec![EntityId(2)]
    );
}
