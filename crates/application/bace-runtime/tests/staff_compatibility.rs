use bace_auth::{AccessLevel, CharacterPrivileges, StaffPrincipal};
use bace_gameplay_api::{ActionContext, SessionId};
use bace_runtime::staff_game_dispatch::{
    PreparedStaffGame, StaffGameInput, prepare_staff_game_command,
};
use bace_types::{AccountId, EntityId};
#[test]
fn excluded_and_source_stub_commands_cannot_reach_target_or_mutation_dispatch() {
    let principal = StaffPrincipal {
        account_access: AccessLevel::Admin,
        character: CharacterPrivileges {
            admin: true,
            ..Default::default()
        },
        in_world: true,
    };
    for (line, error) in [
        ("@allstats", "Incompatible"),
        ("@forcegc", "Incompatible"),
        ("@gcstatus", "Incompatible"),
        ("@clearcache", "Incompatible"),
        ("@deaf player on", "SourceUnimplemented"),
    ] {
        let result = prepare_staff_game_command(
            StaffGameInput {
                line,
                principal,
                context: ActionContext {
                    actor: EntityId(1),
                    account: AccountId(2),
                    session: SessionId(3),
                    sequence: 4,
                },
                token: 5,
                definitions: &[],
                equipment: vec![],
            },
            |_| panic!("rejected command reached target owner"),
        );
        assert!(
            matches!(&result,Err(message) if message.contains(error)),
            "{line}: {}",
            match &result {
                Err(e) => e.as_str(),
                Ok(_) => "unexpected prepared command",
            }
        );
    }
    let pending = prepare_staff_game_command(
        StaffGameInput {
            line: "@createliveops 1",
            principal,
            context: ActionContext {
                actor: EntityId(1),
                account: AccountId(2),
                session: SessionId(3),
                sequence: 4,
            },
            token: 5,
            definitions: &[],
            equipment: vec![],
        },
        |_| panic!("not wired command reached mutation owner"),
    )
    .unwrap();
    assert!(
        matches!(pending,PreparedStaffGame::Other(command) if command.spec.name=="createliveops")
    );
}
