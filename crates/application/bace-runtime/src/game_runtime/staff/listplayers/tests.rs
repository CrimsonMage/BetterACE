use super::*;
use bace_auth::{
    AccountRepository, CharacterPrivileges, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_gameplay_api::CharacterBinding;
use bace_persistence::{CharacterLease, OwnershipState};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV2, PlayerSaveV6};

#[test]
fn pinned_access_names_numeric_enum_and_invalid_source_response() {
    assert_eq!(parse_access_filter(None), Ok(None));
    assert_eq!(
        parse_access_filter(Some("sEnTiNeL")),
        Ok(Some((2, "Sentinel".into())))
    );
    assert_eq!(
        parse_access_filter(Some("4")),
        Ok(Some((4, "Developer".into())))
    );
    assert_eq!(parse_access_filter(Some("6")), Ok(Some((6, "6".into()))));
    assert_eq!(parse_access_filter(Some("-1")), Ok(Some((-1, "-1".into()))));
    assert_eq!(
        parse_access_filter(Some("65536")),
        Ok(Some((65536, "65536".into())))
    );
    for invalid in ["invalid", "2147483648", "0x4"] {
        assert_eq!(parse_access_filter(Some(invalid)), Err(()));
    }
}

#[test]
fn long_source_roster_chunks_only_at_line_boundaries_without_loss() {
    let lines = vec![
        "Listing only Players:\n".into(),
        "Name : 1\n".repeat(500),
        "Total connected Players: 500\n".into(),
    ];
    let chunks = chunk_source_lines(&lines, 4000).unwrap_err();
    assert!(chunks.contains("single line"));
    let lines: Vec<_> = std::iter::once("Listing only Players:\n".to_owned())
        .chain((0..500).map(|i| format!("Name{i} : 1\n")))
        .chain(std::iter::once("Total connected Players: 500\n".to_owned()))
        .collect();
    let chunks = chunk_source_lines(&lines, 4000).unwrap();
    assert!(chunks.len() > 1);
    assert!(chunks.iter().all(|chunk| chunk.len() <= 4000));
    assert_eq!(chunks.concat(), lines.concat());
}

#[tokio::test]
async fn entered_developer_lists_only_bound_online_players_in_private_source_packet() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let CreateAccountOutcome::Created(mut account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: bace_auth::AccountName::parse("listplayers-owner").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("new account")
    };
    account.access_level = AccessLevel::Developer;
    let key = SessionKey {
        id: 1,
        generation: 7,
    };
    runtime.players.authenticated(key, &account).unwrap();
    let actor = bace_types::EntityId(0x5000_0001);
    let binding = CharacterBinding {
        actor,
        account: account.id,
        session: bace_gameplay_api::SessionId(key.generation),
    };
    let player = PlayerSaveV6::migrate_v2(
        PlayerSaveV2::migrate_v1(PlayerSaveV1 {
            entity: EntitySaveV1 {
                object_id: actor.0,
                template_revision: 1,
                mutation_revision: 4,
                state: bace_content::WeenieV1 {
                    schema_version: 1,
                    weenie_id: 1,
                    class_name: "test_player".into(),
                    weenie_type: 1,
                    last_modified: None,
                    properties: Default::default(),
                },
            },
            account_id: account.id.0,
            name: "Test Player".into(),
            metadata: Default::default(),
            quests: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    let loaded = Arc::new(crate::game_login::LoadedPlayer {
        is_plussed: false,
        key,
        binding,
        lease: CharacterLease {
            character_id: actor.0,
            epoch: 1,
            state: OwnershipState::Online,
        },
        persisted_version: 1,
        player,
        cached_experience: 0,
        inventory: vec![],
    });
    runtime
        .players
        .test_admit_replication(key, loaded.clone())
        .unwrap();
    runtime.players.test_mark_entered(key, binding).unwrap();
    runtime.sessions.insert(
        key,
        Session {
            account: account.clone(),
            connected: true,
            closing: false,
            terminated: false,
            disconnected: false,
            loading: Some(lifecycle::Loading {
                loaded: loaded.clone(),
                phase: lifecycle::Phase::Entered,
                account_created: None,
                friends: vec![],
                friends_next: 0,
                cold: None,
                appearance: None,
                character_assets: None,
                enchantments: vec![],
                spell_table: None,
                online: None,
                receipt: None,
                snapshot: None,
                cold_token: None,
                region_requested: false,
                settle_attempts: 0,
                first_settle_tick: None,
            }),
            failure: None,
        },
    );
    let stale = SessionKey {
        id: 2,
        generation: 7,
    };
    runtime.sessions.insert(
        stale,
        Session {
            account: account.clone(),
            connected: true,
            closing: false,
            terminated: false,
            disconnected: false,
            // Same actor and generation with another transport key must not
            // be counted because the admitted replication owner is `key`.
            loading: Some(lifecycle::Loading {
                loaded,
                phase: lifecycle::Phase::Entered,
                account_created: None,
                friends: vec![],
                friends_next: 0,
                cold: None,
                appearance: None,
                character_assets: None,
                enchantments: vec![],
                spell_table: None,
                online: None,
                receipt: None,
                snapshot: None,
                cold_token: None,
                region_requested: false,
                settle_attempts: 0,
                first_settle_tick: None,
            }),
            failure: None,
        },
    );
    let context = ActionContext {
        actor,
        account: account.id,
        session: binding.session,
        sequence: 1,
    };
    let principal = StaffPrincipal {
        account_access: AccessLevel::Developer,
        character: CharacterPrivileges::from_account(AccessLevel::Developer),
        in_world: true,
    };
    let command = bace_admin::authorize_command(
        bace_admin::parse_command("@listplayers Developer").unwrap(),
        Some(principal),
    )
    .unwrap();
    assert!(
        runtime
            .apply_staff_listplayers(key, context, &command, principal)
            .unwrap()
    );
    let Some(NetworkCommand::SendOrderedBatch {
        key: recipient,
        messages,
    }) = runtime.staff.output.pop_front()
    else {
        panic!("private roster packet")
    };
    assert_eq!(recipient, key);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].0, 9);
    let expected = format!(
        "Listing only Developers:\nTest Player : {}\nTotal connected Players: 1\n",
        account.id.0
    );
    assert_eq!(
        messages[0].1,
        bace_wire::ChatMessage::System {
            text: &expected,
            chat_type: 0
        }
        .encode()
        .unwrap()
    );
    assert!(runtime.staff.output.is_empty());
    runtime
        .players
        .test_clear_replication(key, binding)
        .unwrap();
    runtime.sessions.clear();
    runtime.quiesce(Duration::ZERO).unwrap();
}
