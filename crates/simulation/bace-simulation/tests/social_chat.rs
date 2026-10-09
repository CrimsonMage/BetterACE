#[allow(dead_code, unused_imports)]
#[path = "magic_common/mod.rs"]
mod common;
use bace_gameplay_api::social::{
    AllegianceRequest, FellowshipRequest, SocialError, SocialEvent, SocialIdentity, SocialRequest,
    SocialSquelch,
};
use bace_social::{SocialPreferences, SocialPresence};
use common::*;
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

fn setup(gagged: bool) -> Kernel {
    let mut kernel = kernel(32);
    let table = RankTable::new(&[0, 10]).unwrap();
    let character = CharacterProgression::new(
        &[],
        Arc::new(ProgressionTables {
            attributes: table.clone(),
            vitals: table.clone(),
            trained_skills: table.clone(),
            specialized_skills: table,
        }),
        0,
        0,
    )
    .unwrap();
    kernel
        .register_character(
            CharacterBinding {
                session: SessionId(8),
                account: AccountId(2),
                actor: EntityId(2),
            },
            character,
        )
        .unwrap();
    let mut source = presence(1);
    source.gagged = gagged;
    kernel
        .register_social_presence(source, SocialPreferences::default())
        .unwrap();
    kernel
        .register_social_presence(presence(2), SocialPreferences::default())
        .unwrap();
    while kernel.take_social_event().is_some() {}
    kernel
}
fn install_fellowship_services(kernel: &mut Kernel) {
    for actor in [EntityId(1), EntityId(2)] {
        kernel
            .register_npc_character_services(
                actor,
                bace_character::CharacterServiceState {
                    level: 1,
                    total_experience: 0,
                    titles: vec![],
                    enlightenment: 0,
                    sanctuary: None,
                    total_skill_credits: None,
                },
                bace_quests::ContractRegistry::restore(vec![]).unwrap(),
            )
            .unwrap();
    }
}
#[test]
fn gag_outputs_exact_source_pair_without_delivery_or_turbine_ack() {
    let mut k = setup(true);
    assert_eq!(
        k.request_social(
            context(1),
            SocialRequest::Turbine {
                context_id: 8,
                dispatch: 1,
                channel: 2,
                chat_type: 2,
                text: "hello".into()
            }
        ),
        Err(SocialError::Gagged)
    );
    let text =
        "You are unable to talk locally, globally, or send tells because you have been gagged.";
    assert_eq!(
        k.take_social_event(),
        Some(SocialEvent::Transient {
            recipient: EntityId(1),
            text: text.into()
        })
    );
    assert_eq!(
        k.take_social_event(),
        Some(SocialEvent::System {
            recipient: EntityId(1),
            text: text.into(),
            chat_type: 20
        })
    );
    assert!(k.take_social_event().is_none());
}
#[test]
fn tell_reports_offline_and_squelched_after_echo_without_private_delivery() {
    let mut k = setup(false);
    assert_eq!(
        k.request_social(
            context(1),
            SocialRequest::Tell {
                text: "hello".into(),
                target_name: "missing".into()
            }
        ),
        Err(SocialError::Offline)
    );
    assert_eq!(
        k.take_social_event(),
        Some(SocialEvent::Error {
            recipient: EntityId(1),
            code: 0x52b,
            argument: None
        })
    );
    let mut p = k.take_social_presence(EntityId(2)).unwrap();
    p.squelches.push(SocialSquelch {
        character: EntityId(1),
        account: None,
        name: "Player1".into(),
        mask: 8,
    });
    k.register_social_presence(presence(2), p).unwrap();
    while k.take_social_event().is_some() {}
    assert_eq!(
        k.request_social(
            context(2),
            SocialRequest::Tell {
                text: "hello".into(),
                target_name: "Player2".into()
            }
        ),
        Err(SocialError::Squelched)
    );
    assert!(matches!(
        k.take_social_event(),
        Some(SocialEvent::System { chat_type: 4, .. })
    ));
    assert_eq!(
        k.take_social_event(),
        Some(SocialEvent::Error {
            recipient: EntityId(1),
            code: 0x51f,
            argument: Some("Player2 has you squelched.".into())
        })
    );
    assert!(k.take_social_event().is_none());
}
#[test]
fn turbine_adjusts_name_channel_and_rejects_nonmember_society_without_ack() {
    let mut k = setup(false);
    k.request_social(
        context(1),
        SocialRequest::Turbine {
            context_id: 8,
            dispatch: 1,
            channel: 3,
            chat_type: 2,
            text: "hello".into(),
        },
    )
    .unwrap();
    assert!(matches!(
        k.take_social_event(),
        Some(SocialEvent::Chat {
            wire: bace_gameplay_api::social::ChatDelivery::Turbine {
                channel: 2,
                chat_type: 2
            },
            ..
        })
    ));
    assert!(matches!(
        k.take_social_event(),
        Some(SocialEvent::TurbineResponse { context_id: 8, .. })
    ));
    assert_eq!(
        k.request_social(
            context(2),
            SocialRequest::Turbine {
                context_id: 9,
                dispatch: 1,
                channel: 2,
                chat_type: 6,
                text: "hello".into()
            }
        ),
        Err(SocialError::Forbidden)
    );
    assert_eq!(
        k.take_social_event(),
        Some(SocialEvent::System {
            recipient: EntityId(1),
            text: "You do not belong to a society.".into(),
            chat_type: 0
        })
    );
    assert!(k.take_social_event().is_none());
}

#[test]
fn age_gate_echo_is_not_accepted_chat_and_acks_after_source_notice_pair() {
    let mut k = kernel(32);
    k.configure_chat_policy(bace_social::ChatPolicy {
        requires_account_time_seconds: 100,
        echo_reject: true,
        ..Default::default()
    })
    .unwrap();
    k.register_social_presence(presence(1), SocialPreferences::default())
        .unwrap();
    k.register_chat_eligibility(EntityId(1), Default::default())
        .unwrap();
    while k.take_social_event().is_some() {}
    k.request_social(
        context(1),
        SocialRequest::Turbine {
            context_id: 8,
            dispatch: 1,
            channel: 2,
            chat_type: 2,
            text: "hello".into(),
        },
    )
    .unwrap();
    assert!(matches!(
        k.take_social_event(),
        Some(SocialEvent::TurbineEcho { channel: 2, .. })
    ));
    assert!(matches!(
        k.take_social_event(),
        Some(SocialEvent::Transient { .. })
    ));
    assert!(matches!(
        k.take_social_event(),
        Some(SocialEvent::System { chat_type: 0, .. })
    ));
    assert!(matches!(
        k.take_social_event(),
        Some(SocialEvent::TurbineResponse { context_id: 8, .. })
    ));
    assert!(k.take_social_event().is_none());
}

#[test]
fn resolved_offline_friend_is_cached_after_authorization_and_account_squelch_uses_account() {
    let mut k = setup(false);
    let identity = SocialIdentity {
        character: EntityId(0x50000010),
        account: AccountId(77),
        name: "Offline".into(),
    };
    k.request_social_resolved(
        context(1),
        SocialRequest::AddFriend("oFfLiNe".into()),
        Some(identity.clone()),
    )
    .unwrap();
    assert!(
        k.social_preferences(EntityId(1))
            .unwrap()
            .friends
            .contains(&identity.character)
    );
    assert!(
        matches!(k.take_social_event(),Some(SocialEvent::Friends{kind:1,entries,..}) if !entries[0].online)
    );
    k.request_social_resolved(
        context(2),
        SocialRequest::AccountSquelch {
            enabled: true,
            name: "Offline".into(),
        },
        Some(identity.clone()),
    )
    .unwrap();
    assert_eq!(
        k.social_preferences(EntityId(1)).unwrap().squelches[0].account,
        Some(AccountId(77))
    );
    let before = k.character(EntityId(1)).unwrap().revision();
    assert_eq!(
        k.request_social_resolved(
            context(3),
            SocialRequest::AddFriend("Different".into()),
            Some(identity)
        ),
        Err(SocialError::Stale)
    );
    assert_eq!(k.character(EntityId(1)).unwrap().revision(), before);
    let mut stale = context(4);
    stale.session = SessionId(999);
    assert_eq!(
        k.request_social_resolved(
            stale,
            SocialRequest::AddFriend("Secret".into()),
            Some(SocialIdentity {
                character: EntityId(0x50000011),
                account: AccountId(78),
                name: "Secret".into()
            })
        ),
        Err(SocialError::Stale)
    );
}

fn allegiance_node(id: u32) -> bace_allegiance::AllegianceNode {
    bace_allegiance::AllegianceNode {
        character: EntityId(id),
        account: AccountId(id.into()),
        name: format!("Player{id}"),
        gender: 1,
        heritage: 1,
        patron: (id != 2).then_some(EntityId(2)),
        monarch: EntityId(2),
        vassals: if id == 2 {
            vec![EntityId(1), EntityId(3)]
        } else {
            vec![]
        },
        rank: if id == 2 { 2 } else { 1 },
        followers: if id == 2 { 2 } else { 0 },
        level: 1,
        leadership: 0,
        loyalty: 0,
        sworn_at: 0,
        online_seconds: 0,
        may_pass_up: true,
        received_total: 0,
        tithed_total: 0,
        unclaimed: 0,
    }
}
#[test]
fn original_covassal_patron_channel_and_source_self_echo() {
    let mut rows = 0;
    for line in include_str!("../../../gameplay/bace-social/tests/fixtures/legacy_chat.csv")
        .lines()
        .filter(|s| s.starts_with("C,"))
    {
        let p: Vec<_> = line.split(',').collect();
        let mask: u32 = p[1].parse().unwrap();
        let online: u32 = p[2].parse().unwrap();
        if online & 4 != 0 {
            continue;
        }
        let mut k = setup(false);
        let registry = bace_allegiance::AllegianceRegistry::restore(
            (1..=3).map(allegiance_node).collect(),
            vec![bace_allegiance::AllegianceMetadata::new(EntityId(2), 100)],
            1,
            32,
        )
        .unwrap();
        k.register_allegiances(registry, 0.0).unwrap();
        if mask & 2 != 0 {
            k.request_social(
                bace_gameplay_api::ActionContext {
                    actor: EntityId(2),
                    account: AccountId(2),
                    session: SessionId(8),
                    sequence: 1,
                },
                SocialRequest::GlobalSquelch {
                    enabled: true,
                    message_type: 18,
                },
            )
            .unwrap();
        }
        if online & 2 == 0 {
            let mut p = presence(2);
            p.online = false;
            k.refresh_social_presence(p).unwrap();
        }
        while k.take_social_event().is_some() {}
        k.request_social(
            context(1),
            SocialRequest::Channel {
                channel: 0x1000000,
                text: "hello".into(),
            },
        )
        .unwrap();
        let mut actual = vec![];
        while let Some(event) = k.take_social_event() {
            if let SocialEvent::Chat {
                wire: bace_gameplay_api::social::ChatDelivery::LegacyChannel(channel),
                recipients,
                ..
            } = event
            {
                for recipient in recipients {
                    actual.push(format!(
                        "{}:{channel}:{}",
                        recipient.0,
                        u8::from(recipient != EntityId(1))
                    ));
                }
            }
        }
        assert_eq!(actual.join(";"), p[3], "{line}");
        rows += 1;
    }
    assert_eq!(rows, 16);
}
#[test]
fn unchanged_global_squelch_only_reports_source_notice() {
    let mut k = setup(false);
    let request = SocialRequest::GlobalSquelch {
        enabled: true,
        message_type: 18,
    };
    k.request_social(context(1), request.clone()).unwrap();
    while k.take_social_event().is_some() {}
    k.request_social(context(2), request).unwrap();
    assert!(
        matches!(k.take_social_event(),Some(SocialEvent::System{text,..}) if text=="The Allegiance channel is already squelched.")
    );
    assert!(k.take_social_event().is_none());
}

#[test]
fn direct_presence_handoff_leaves_fellowship_and_preserves_ordered_departure_output() {
    // Pinned ACE Player.cs::LogOut_Inner invokes FellowshipQuit(false) before
    // removing the player's live presence.
    let mut k = setup(false);
    install_fellowship_services(&mut k);
    let mut second = presence(2);
    second.auto_accept_fellowship = true;
    k.refresh_social_presence(second).unwrap();
    k.request_fellowship(
        context(1),
        FellowshipRequest::Create {
            name: "Travelers".into(),
            share_xp: true,
        },
    )
    .unwrap();
    k.request_fellowship(context(2), FellowshipRequest::Recruit(EntityId(2)))
        .unwrap();
    while k.take_social_event().is_some() {}

    assert!(k.take_social_presence(EntityId(2)).is_ok());
    assert!(matches!(
        k.take_social_event(),
        Some(SocialEvent::FellowshipLeft {
            recipients,
            actor: EntityId(2),
            dismissed: false,
            disbanded: false,
        }) if recipients == vec![EntityId(1), EntityId(2)]
    ));
    assert!(matches!(
        k.take_social_event(),
        Some(SocialEvent::Fellowship { recipients, snapshot })
            if recipients == vec![EntityId(1)] && snapshot.members.len() == 1
                && snapshot.members[0].actor == EntityId(1)
    ));
    assert!(k.take_social_event().is_none());

    k.register_social_presence(presence(2), SocialPreferences::default())
        .unwrap();
    k.request_fellowship(
        ActionContext {
            actor: EntityId(2),
            account: AccountId(2),
            session: SessionId(8),
            sequence: 3,
        },
        FellowshipRequest::Create {
            name: "New group".into(),
            share_xp: false,
        },
    )
    .unwrap();
}

#[test]
fn departure_cancels_invitations_from_both_sides_and_update_listener() {
    let mut k = setup(false);
    install_fellowship_services(&mut k);
    k.request_fellowship(
        context(1),
        FellowshipRequest::Create {
            name: "Travelers".into(),
            share_xp: false,
        },
    )
    .unwrap();
    k.request_fellowship(context(2), FellowshipRequest::Recruit(EntityId(2)))
        .unwrap();
    k.request_allegiance(context(3), AllegianceRequest::Update { enabled: true })
        .unwrap();
    while k.take_social_event().is_some() {}

    let saved = k.take_social_presence(EntityId(1)).unwrap();
    assert!(k.take_social_presence(EntityId(1)).is_err());
    k.register_social_presence(presence(1), saved).unwrap();
    k.request_fellowship(
        context(4),
        FellowshipRequest::Create {
            name: "Again".into(),
            share_xp: false,
        },
    )
    .unwrap();
    // The prior invitation may not reserve the recipient's confirmation slot.
    k.request_fellowship(context(5), FellowshipRequest::Recruit(EntityId(2)))
        .unwrap();
}

#[test]
fn disband_with_shared_loot_sends_source_notice_after_departure_to_each_member() {
    // Pinned ACE Entity/Fellowship.cs::QuitFellowship(disband=true) emits
    // GameEventFellowshipDisband and then this Broadcast chat to each fellow.
    let mut k = setup(false);
    install_fellowship_services(&mut k);
    let mut leader = presence(1);
    leader.share_fellowship_loot = true;
    k.refresh_social_presence(leader).unwrap();
    let mut second = presence(2);
    second.auto_accept_fellowship = true;
    k.refresh_social_presence(second).unwrap();
    k.request_fellowship(
        context(1),
        FellowshipRequest::Create {
            name: "Travelers".into(),
            share_xp: false,
        },
    )
    .unwrap();
    k.request_fellowship(context(2), FellowshipRequest::Recruit(EntityId(2)))
        .unwrap();
    while k.take_social_event().is_some() {}

    k.request_fellowship(context(3), FellowshipRequest::Quit { disband: true })
        .unwrap();
    assert_eq!(
        k.take_social_event(),
        Some(SocialEvent::FellowshipLeft {
            recipients: vec![EntityId(1), EntityId(2)],
            actor: EntityId(1),
            dismissed: false,
            disbanded: true,
        })
    );
    for recipient in [EntityId(1), EntityId(2)] {
        assert_eq!(
            k.take_social_event(),
            Some(SocialEvent::System {
                recipient,
                text: "You no longer have permission to loot anyone else's kills.".into(),
                chat_type: 0,
            })
        );
    }
    assert!(k.take_social_event().is_none());
    assert!(k.take_social_presence(EntityId(1)).is_ok());
    assert!(k.take_social_presence(EntityId(2)).is_ok());
}
