use bace_admin::{
    CommandError, authorize_command,
    command_catalog::{command, commands},
    parse_command,
};
use bace_auth::{AccessLevel, CharacterPrivileges, StaffPrincipal};

#[test]
fn pinned_sentinel_ban_family_keeps_lookup_before_duration_validation() {
    let principal = StaffPrincipal {
        account_access: AccessLevel::Sentinel,
        character: CharacterPrivileges::from_account(AccessLevel::Sentinel),
        in_world: true,
    };
    let ban = authorize_command(
        parse_command("@ban AccountName invalid 0 5 repeated abuse").unwrap(),
        Some(principal),
    )
    .unwrap();
    assert_eq!(ban.spec.handler, "HandleBanAccount");
    assert!(!ban.spec.requires_world);
    let Some(bace_admin::StaffOperation::Ban {
        name,
        duration,
        reason,
    }) = bace_admin::prepare_staff_operation(&ban).unwrap()
    else {
        panic!("typed ban");
    };
    assert_eq!(name, "AccountName");
    assert_eq!(duration, ["invalid", "0", "5"]);
    assert_eq!(reason.as_deref(), Some("repeated abuse"));
    for (line, handler) in [
        ("@unban AccountName", "HandleUnBanAccount"),
        ("@banlist", "HandleBanlist"),
    ] {
        let command = authorize_command(parse_command(line).unwrap(), Some(principal)).unwrap();
        assert_eq!(command.spec.handler, handler);
        assert_eq!(command.spec.access, AccessLevel::Sentinel);
        assert!(!command.spec.requires_world);
    }
}

#[test]
fn pinned_envoy_myiid_is_a_world_bound_identity_inspection() {
    // ACE AdminCommands.HandleMyIID is Envoy, RequiresWorld, and sends the
    // issuer's ObjectGuid components. A catalog row alone is not execution.
    let principal = StaffPrincipal {
        account_access: AccessLevel::Envoy,
        character: CharacterPrivileges::from_account(AccessLevel::Envoy),
        in_world: true,
    };
    let command = authorize_command(parse_command("@myiid").unwrap(), Some(principal)).unwrap();
    assert_eq!(command.spec.handler, "HandleMyIID");
    assert_eq!(command.spec.access, AccessLevel::Envoy);
    assert!(command.spec.requires_world);
    assert!(matches!(
        bace_admin::prepare_staff_operation(&command).unwrap(),
        Some(bace_admin::StaffOperation::Inspect {
            target: bace_admin::StaffTarget::SelfActor,
            kind: bace_admin::Inspection::Identity,
        })
    ));
    assert!(matches!(
        authorize_command(
            parse_command("@myiid").unwrap(),
            Some(StaffPrincipal {
                in_world: false,
                ..principal
            })
        ),
        Err(CommandError::NotInWorld)
    ));
}

#[test]
fn pinned_developer_myloc_keeps_its_three_line_owner_distinct_from_targetloc() {
    // ACE DeveloperCommands.HandleMyLoc writes CurrentLandblock, Location,
    // then Physics from the issuer. HandleTargetLoc has a separate selection.
    let principal = StaffPrincipal {
        account_access: AccessLevel::Developer,
        character: CharacterPrivileges::from_account(AccessLevel::Developer),
        in_world: true,
    };
    let own = authorize_command(parse_command("@myloc").unwrap(), Some(principal)).unwrap();
    assert_eq!(own.spec.handler, "HandleMyLoc");
    assert!(matches!(
        bace_admin::prepare_staff_operation(&own).unwrap(),
        Some(bace_admin::StaffOperation::Inspect {
            target: bace_admin::StaffTarget::SelfActor,
            kind: bace_admin::Inspection::SelfPosition,
        })
    ));
    let selected =
        authorize_command(parse_command("@targetloc").unwrap(), Some(principal)).unwrap();
    assert!(matches!(
        bace_admin::prepare_staff_operation(&selected).unwrap(),
        Some(bace_admin::StaffOperation::Inspect {
            target: bace_admin::StaffTarget::Selected,
            kind: bace_admin::Inspection::Position,
        })
    ));
}

#[test]
fn pinned_developer_whoami_is_a_distinct_world_bound_self_inspection() {
    let principal = StaffPrincipal {
        account_access: AccessLevel::Developer,
        character: CharacterPrivileges::from_account(AccessLevel::Developer),
        in_world: true,
    };
    let command = authorize_command(parse_command("@whoami").unwrap(), Some(principal)).unwrap();
    assert_eq!(command.spec.handler, "HandleWhoAmI");
    assert_eq!(command.spec.access, AccessLevel::Developer);
    assert!(command.spec.requires_world);
    assert!(matches!(
        bace_admin::prepare_staff_operation(&command).unwrap(),
        Some(bace_admin::StaffOperation::Inspect {
            target: bace_admin::StaffTarget::SelfActor,
            kind: bace_admin::Inspection::WhoAmI,
        })
    ));
    assert!(matches!(
        authorize_command(
            parse_command("@whoami").unwrap(),
            Some(StaffPrincipal {
                in_world: false,
                ..principal
            })
        ),
        Err(CommandError::NotInWorld)
    ));
}

#[test]
fn pinned_developer_listplayers_retains_optional_access_filter() {
    let principal = StaffPrincipal {
        account_access: AccessLevel::Developer,
        character: CharacterPrivileges::from_account(AccessLevel::Developer),
        in_world: true,
    };
    for (line, expected) in [
        ("@listplayers", None),
        ("@listplayers Sentinel", Some("Sentinel")),
        ("@listplayers 6", Some("6")),
    ] {
        let command = authorize_command(parse_command(line).unwrap(), Some(principal)).unwrap();
        assert_eq!(command.spec.handler, "HandleListPlayers");
        assert_eq!(command.spec.access, AccessLevel::Developer);
        assert!(!command.spec.requires_world);
        let Some(bace_admin::StaffOperation::ListPlayers { access }) =
            bace_admin::prepare_staff_operation(&command).unwrap()
        else {
            panic!("typed listplayers")
        };
        assert_eq!(access.as_deref(), expected);
    }
}

#[test]
fn pinned_developer_gps_is_a_distinct_authoritative_self_inspection() {
    let principal = StaffPrincipal {
        account_access: AccessLevel::Developer,
        character: CharacterPrivileges::from_account(AccessLevel::Developer),
        in_world: true,
    };
    let command = authorize_command(parse_command("@gps").unwrap(), Some(principal)).unwrap();
    assert_eq!(command.spec.handler, "HandleDebugGPS");
    assert_eq!(command.spec.access, AccessLevel::Developer);
    assert!(command.spec.requires_world);
    assert!(matches!(
        bace_admin::prepare_staff_operation(&command).unwrap(),
        Some(bace_admin::StaffOperation::Inspect {
            target: bace_admin::StaffTarget::SelfActor,
            kind: bace_admin::Inspection::Gps,
        })
    ));
}

#[test]
fn pinned_envoy_time_is_a_typed_read_only_command_without_world_requirement() {
    // ACE AdminCommands.HandleTime has CommandHandlerFlag.None and prints
    // UTC, in-game Dereth date, and TimeOfDay as three WorldBroadcast lines.
    let principal = StaffPrincipal {
        account_access: AccessLevel::Envoy,
        character: CharacterPrivileges::from_account(AccessLevel::Envoy),
        in_world: true,
    };
    let command = authorize_command(parse_command("@time").unwrap(), Some(principal)).unwrap();
    assert_eq!(command.spec.handler, "HandleTime");
    assert_eq!(command.spec.access, AccessLevel::Envoy);
    assert!(!command.spec.requires_world);
    assert!(matches!(
        bace_admin::prepare_staff_operation(&command).unwrap(),
        Some(bace_admin::StaffOperation::Time)
    ));
}

#[test]
fn pinned_envoy_regen_requires_world_and_the_source_selection_order() {
    // ACE AdminCommands.HandleRegen uses HealthQueryTarget, ManaQueryTarget,
    // CurrentAppraisalTarget in that order; the runtime resolves the variant.
    let principal = StaffPrincipal {
        account_access: AccessLevel::Envoy,
        character: CharacterPrivileges::from_account(AccessLevel::Envoy),
        in_world: true,
    };
    let command = authorize_command(parse_command("@regen").unwrap(), Some(principal)).unwrap();
    assert_eq!(command.spec.handler, "HandleRegen");
    assert_eq!(command.spec.access, AccessLevel::Envoy);
    assert!(command.spec.requires_world);
    assert!(matches!(
        bace_admin::prepare_staff_operation(&command).unwrap(),
        Some(bace_admin::StaffOperation::Regenerate {
            target: bace_admin::StaffTarget::SelectedGenerator
        })
    ));
}

#[test]
fn pinned_sentinel_boot_parses_source_selector_and_comma_reason() {
    // ACE SentinelCommands.HandleBoot joins parameters after the selector,
    // then splits the optional reason at its first comma.
    let principal = StaffPrincipal {
        account_access: AccessLevel::Sentinel,
        character: CharacterPrivileges::from_account(AccessLevel::Sentinel),
        in_world: true,
    };
    let command = authorize_command(
        parse_command("@boot char Some Player, repeated abuse").unwrap(),
        Some(principal),
    )
    .unwrap();
    assert_eq!(command.spec.handler, "HandleBoot");
    assert_eq!(command.spec.access, AccessLevel::Sentinel);
    assert!(!command.spec.requires_world);
    let Some(bace_admin::StaffOperation::Boot {
        selector,
        name,
        reason,
    }) = bace_admin::prepare_staff_operation(&command).unwrap()
    else {
        panic!("boot needs a typed owner")
    };
    assert_eq!(selector, bace_admin::BootSelector::Character);
    assert_eq!(name, "Some Player");
    assert_eq!(reason.as_deref(), Some("repeated abuse"));
    assert!(matches!(
        authorize_command(
            parse_command("@boot account SomePlayer").unwrap(),
            Some(StaffPrincipal {
                account_access: AccessLevel::Player,
                character: CharacterPrivileges::default(),
                in_world: true,
            })
        ),
        Err(CommandError::NotAuthorized)
    ));
}
#[test]
fn broadcast_alias_conversion_preserves_source_privilege_and_newline_mode() {
    let principal = StaffPrincipal {
        account_access: AccessLevel::Admin,
        character: CharacterPrivileges {
            admin: true,
            ..Default::default()
        },
        in_world: true,
    };
    for (name, emote, local) in [
        ("gamecast", false, false),
        ("gamecastlocal", false, true),
        ("gamecastemote", true, false),
        ("gamecastlocalemote", true, true),
        ("we", true, false),
    ] {
        let command = authorize_command(
            parse_command(&format!("@{name} one\\ntwo")).unwrap(),
            Some(principal),
        )
        .unwrap();
        let Some(bace_admin::StaffOperation::Broadcast {
            text,
            emote: actual_emote,
            local: actual_local,
        }) = bace_admin::prepare_staff_operation(&command).unwrap()
        else {
            panic!("missing typed broadcast")
        };
        assert_eq!((actual_emote, actual_local), (emote, local));
        assert_eq!(text, "one\\ntwo");
        assert_eq!(
            command.spec.access,
            if name == "gamecast" {
                AccessLevel::Envoy
            } else {
                AccessLevel::Developer
            }
        );
    }
}
#[test]
fn full_pinned_catalog_has_distinct_names_and_source_metadata() {
    let all: Vec<_> = commands().collect();
    assert_eq!(all.len(), 321);
    let names: std::collections::BTreeSet<_> = all.iter().map(|c| c.name).collect();
    assert_eq!(names.len(), 321);
    let mut levels = [0; 6];
    for c in all {
        levels[c.access as usize] += 1;
        assert!(!c.source.is_empty());
        assert!(!c.handler.is_empty());
    }
    assert_eq!(levels, [14, 11, 27, 16, 187, 66]);
    assert_eq!(command("heal").unwrap().access, AccessLevel::Envoy);
    assert!(command("deaf").unwrap().source_stub);
    assert_eq!(command("teleloc").unwrap().min_args, 4);
}
#[test]
fn quoted_parser_authority_and_console_world_gates() {
    let p = parse_command("@rename \"Some Person\" \"New Name\"").unwrap();
    assert_eq!(p.arguments, ["Some Person", "New Name"]);
    assert_eq!(
        parse_command("@rename \"unterminated").unwrap_err(),
        CommandError::UnterminatedQuote
    );
    let mut principal = StaffPrincipal {
        account_access: AccessLevel::Admin,
        character: CharacterPrivileges::default(),
        in_world: true,
    };
    assert!(matches!(
        authorize_command(parse_command("heal").unwrap(), Some(principal)),
        Err(CommandError::NotAuthorized)
    ));
    assert!(authorize_command(parse_command("sudo heal").unwrap(), Some(principal)).is_ok());
    principal.account_access = AccessLevel::Player;
    principal.character.admin = true;
    assert!(matches!(
        authorize_command(parse_command("sudo heal").unwrap(), Some(principal)),
        Err(CommandError::NotAuthorized)
    ));
    assert!(authorize_command(parse_command("heal").unwrap(), Some(principal)).is_ok());
    assert!(matches!(
        authorize_command(parse_command("heal").unwrap(), None),
        Err(CommandError::NotInWorld)
    ));
    assert!(matches!(
        authorize_command(parse_command("exit").unwrap(), Some(principal)),
        Err(CommandError::ConsoleOnly)
    ));
}
#[test]
fn original_csharp_attribute_and_parser_fixture() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/commands.json")).unwrap();
    let declarations = fixture["declarations"].as_array().unwrap();
    assert_eq!(declarations.len(), 324);
    for spec in commands() {
        assert!(
            declarations.iter().any(|r| r["name"] == spec.name
                && r["access"] == spec.access as u8
                && r["min_args"] == spec.min_args
                && r["include_raw"] == spec.include_raw
                && r["description"] == spec.description
                && r["usage"] == spec.usage
                && r["flags"]
                    == (u8::from(spec.console_only) | (u8::from(spec.requires_world) * 2))),
            "{}",
            spec.name
        );
    }
    for row in fixture["parses"].as_array().unwrap() {
        let parsed = parse_command(row["input"].as_str().unwrap()).unwrap();
        assert_eq!(parsed.name, row["name"].as_str().unwrap());
        assert_eq!(serde_json::json!(parsed.arguments), row["arguments"]);
    }
}
#[test]
fn typed_commands_preserve_source_position_order_and_redact_passwords() {
    use bace_admin::{StaffOperation, TeleportOperation, prepare_staff_operation};
    let principal = Some(StaffPrincipal {
        account_access: AccessLevel::Admin,
        character: CharacterPrivileges::from_account(AccessLevel::Admin),
        in_world: true,
    });
    let command = authorize_command(
        parse_command("teleloc 12340001 [1 2 3] 0.5 0.1 0.2 0.3").unwrap(),
        principal,
    )
    .unwrap();
    let StaffOperation::Teleport(TeleportOperation::Location(p)) =
        prepare_staff_operation(&command).unwrap().unwrap()
    else {
        panic!()
    };
    assert_eq!(p.cell, 0x12340001);
    assert_eq!(p.origin, [1., 2., 3.]);
    assert_eq!(p.rotation, [0.5, 0.1, 0.2, 0.3]);
    let command = authorize_command(
        parse_command("telexyz 305397761 1 2 3 0.1 0.2 0.3 0.5").unwrap(),
        principal,
    )
    .unwrap();
    let StaffOperation::Teleport(TeleportOperation::Location(q)) =
        prepare_staff_operation(&command).unwrap().unwrap()
    else {
        panic!()
    };
    assert_eq!(p, q);
    let command = authorize_command(
        parse_command("set-accountpassword test sentinel-secret").unwrap(),
        principal,
    )
    .unwrap();
    assert!(!format!("{command:?}").contains("sentinel-secret"));
    assert!(
        !format!("{:?}", prepare_staff_operation(&command).unwrap()).contains("sentinel-secret")
    );
}
