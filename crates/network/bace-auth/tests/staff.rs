use bace_auth::{AccessLevel, CharacterPrivileges, StaffPrincipal};
#[test]
fn account_character_and_map_roles_remain_distinct() {
    let levels = [
        AccessLevel::Player,
        AccessLevel::Advocate,
        AccessLevel::Sentinel,
        AccessLevel::Envoy,
        AccessLevel::Developer,
        AccessLevel::Admin,
    ];
    for (index, level) in levels.iter().copied().enumerate() {
        let flags = CharacterPrivileges::from_account(level);
        for (required, minimum) in levels.iter().copied().enumerate() {
            assert_eq!(flags.allows(minimum), index >= required);
        }
        assert_eq!(flags.map_teleport(), index >= 4);
        let principal = StaffPrincipal {
            account_access: level,
            character: CharacterPrivileges::default(),
            in_world: true,
        };
        assert_eq!(principal.allows(AccessLevel::Sentinel, true), index >= 2);
        assert!(!principal.allows(AccessLevel::Sentinel, false));
    }
    let mut advocate = CharacterPrivileges::from_account(AccessLevel::Advocate);
    advocate.psr = true;
    assert!(advocate.map_teleport());
    assert!(!advocate.allows(AccessLevel::Sentinel));
    assert!(CharacterPrivileges::from_account(AccessLevel::Envoy).sentinel);
}
