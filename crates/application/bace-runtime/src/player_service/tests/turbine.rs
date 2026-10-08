use super::*;

#[tokio::test]
async fn turbine_roster_capability_requires_the_composed_social_owner() {
    let cluster = cluster::Cluster::start();
    let store = bace_db_postgres::PgStore::connect(&cluster.url(), 2)
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("turbine-capability").unwrap(),
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
    let mut service = PlayerService::new(GameLoginService::new(store, 2, 11).unwrap(), 4).unwrap();
    let key = SessionKey {
        id: 1,
        generation: 1,
    };
    service.authenticated(key, &account).unwrap();
    for enabled in [false, true] {
        if enabled {
            service.enable_turbine_chat();
        }
        let result = service
            .begin_io(key, PlayerIoAction::Roster)
            .unwrap()
            .execute()
            .await;
        assert!(matches!(
            service
                .accept_io(result)
                .unwrap_or_else(|_| panic!("I/O correlation")),
            Ok(PlayerIoResult::Roster)
        ));
        let NetworkCommand::Send { bytes, .. } = service.network.pop_front().unwrap() else {
            panic!("roster command")
        };
        assert_eq!(
            bace_wire::CharacterList::decode(&bytes, 11, 128)
                .unwrap()
                .use_turbine_chat,
            enabled
        );
    }
}
