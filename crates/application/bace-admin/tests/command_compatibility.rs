use bace_admin::{
    CommandCompatibility as C, CommandError, authorize_command, command_catalog::command,
    command_compatibility, parse_command,
};
#[test]
fn incompatible_source_implementations_fail_explicitly_without_a_pending_owner() {
    for name in [
        "allstats",
        "forcegc",
        "forcegc2",
        "gcstatus",
        "clearphysicscaches",
        "clearcache",
        "database-shard-cache-pbrt",
        "database-shard-cache-npbrt",
        "databaseperftest",
        "import-json",
        "import-sql",
        "import-sql-folders",
        "export-json",
        "export-json-folders",
        "export-sql",
        "export-sql-folders",
        "fix-allegiances",
        "fix-biota-emote-delay",
        "fix-gear-plating",
        "fix-shortcut-bars",
        "fix-spell-bars",
    ] {
        let spec = command(name).unwrap();
        assert!(!spec.source_stub);
        let C::Incompatible(reason) = command_compatibility(spec) else {
            panic!("missing source incompatibility: {name}")
        };
        assert!(reason.len() > 40);
    }
    assert!(matches!(
        authorize_command(parse_command("forcegc").unwrap(), None),
        Err(CommandError::Incompatible(_))
    ));
}
#[test]
fn valid_native_adaptations_and_source_todos_are_not_conflated() {
    for name in [
        "createliveops",
        "create",
        "databasequeueinfo",
        "gag",
        "ungag",
        "gamecast",
        "verify-player-data",
        "copychar",
    ] {
        assert_eq!(
            command_compatibility(command(name).unwrap()),
            C::NativeOwnerRequired
        );
    }
    assert_eq!(
        command_compatibility(command("deaf").unwrap()),
        C::SourceTodo
    );
}
