//! Catalog-authorized typed dispatch; live Kernel staff authority is rechecked.
//! Remaining owner families retain the authorized request rather than report success.
use bace_admin::{
    AuthorizedCommand, Inspection, StaffOperation, StaffTarget, authorize_command, parse_command,
    prepare_staff_operation,
};
use bace_auth::StaffPrincipal;
use bace_gameplay_api::{
    ActionContext,
    staff::{StaffAction, StaffBaneItem, StaffCommand, StaffInspection, StaffSpellDefinition},
};
use bace_types::EntityId;
pub struct StaffGameInput<'a> {
    pub line: &'a str,
    pub principal: StaffPrincipal,
    pub context: ActionContext,
    pub token: u64,
    pub definitions: &'a [StaffSpellDefinition],
    pub equipment: Vec<StaffBaneItem>,
}
pub enum PreparedStaffGame {
    Simulation(Box<StaffCommand>),
    Other(AuthorizedCommand),
}
pub fn prepare_staff_game_command(
    input: StaffGameInput<'_>,
    mut resolve: impl FnMut(StaffTarget) -> Result<EntityId, String>,
) -> Result<PreparedStaffGame, String> {
    if input.token == 0 || input.equipment.len() > 4096 {
        return Err("staff command identity/capacity".into());
    }
    let authorized = authorize_command(
        parse_command(input.line).map_err(|e| format!("staff syntax: {e:?}"))?,
        Some(input.principal),
    )
    .map_err(|e| format!("staff authorization: {e:?}"))?;
    let Some(operation) =
        prepare_staff_operation(&authorized).map_err(|e| format!("staff parameters: {e:?}"))?
    else {
        return Ok(PreparedStaffGame::Other(authorized));
    };
    let context = input.context;
    let sudo = authorized.sudo;
    let action = match operation {
        StaffOperation::Broadcast { text, emote, local } => StaffAction::Broadcast {
            context,
            text,
            emote,
            local,
            sudo,
        },
        StaffOperation::GrantExperience { target, amount } => StaffAction::GrantExperience {
            context,
            target: resolve(target)?,
            amount,
            sudo,
        },
        StaffOperation::Regenerate { target } => StaffAction::Regenerate {
            context,
            target: resolve(target)?,
            sudo,
        },
        StaffOperation::Run(mode) => StaffAction::Run {
            context,
            mode,
            sudo,
        },
        StaffOperation::CastSpell { spell, target } => {
            let definition = input
                .definitions
                .iter()
                .find(|d| d.spell == spell)
                .ok_or("spell not found")?;
            let target = if definition.targeted {
                Some(resolve(target)?)
            } else {
                None
            };
            StaffAction::CastSpell {
                context,
                target,
                spell,
                sudo,
            }
        }
        StaffOperation::Buff {
            target,
            maximum_level,
            fellowship,
        } => StaffAction::Buff {
            context,
            target: resolve(target)?,
            fellowship,
            maximum_level,
            equipment: input.equipment,
            sudo,
        },
        StaffOperation::AddSpell { spell } | StaffOperation::RemoveSpell { spell } => {
            let learn = authorized.spec.name == "addspell";
            let spell = crate::staff_magic_assets::resolve_staff_spell_name(
                &spell,
                input.definitions,
                learn,
            )
            .ok_or("invalid spell name")?;
            StaffAction::Spellbook {
                context,
                spell,
                learn,
                sudo,
            }
        }
        StaffOperation::Heal { target } => StaffAction::Heal {
            context,
            target: resolve(target)?,
            target_name: None,
            sudo,
        },
        StaffOperation::Inspect { target, kind } => StaffAction::Inspect {
            context,
            target: resolve(target)?,
            kind: match kind {
                Inspection::Identity => StaffInspection::Identity,
                Inspection::WhoAmI => StaffInspection::WhoAmI,
                Inspection::SelfPosition => StaffInspection::SelfPosition,
                Inspection::Gps => StaffInspection::Gps,
                Inspection::Position => StaffInspection::Position,
                Inspection::Enchantments => StaffInspection::Enchantments,
                Inspection::Vitals => StaffInspection::Vitals,
            },
            sudo,
        },
        _ => return Ok(PreparedStaffGame::Other(authorized)),
    };
    Ok(PreparedStaffGame::Simulation(Box::new(StaffCommand {
        token: input.token,
        action,
    })))
}
/// Source (WeenieType.Clothing || IsShield) && ResistMagic < 9999.
/// Caller ties this immutable metadata to the accepted revision; Kernel verifies it.
pub fn prepare_staff_bane_item(
    item: EntityId,
    revision: u64,
    source: &bace_content::WeenieV1,
) -> Result<StaffBaneItem, String> {
    if item.0 == 0 {
        return Err("invalid buff equipment identity".into());
    }
    let value = |id| {
        source
            .properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map_or(0, |p| p.value)
    };
    Ok(StaffBaneItem {
        item,
        revision,
        template: source.weenie_id,
        eligible: (source.weenie_type == 2 || value(51) == 4) && value(36) < 9999,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_auth::{AccessLevel, CharacterPrivileges};
    use bace_gameplay_api::SessionId;
    use bace_types::AccountId;

    #[test]
    fn developer_myloc_dispatches_only_the_authoritative_self_inspection() {
        let context = ActionContext {
            actor: EntityId(0x5000_0001),
            account: AccountId(7),
            session: SessionId(9),
            sequence: 11,
        };
        let prepared = prepare_staff_game_command(
            StaffGameInput {
                line: "@myloc",
                principal: StaffPrincipal {
                    account_access: AccessLevel::Developer,
                    character: CharacterPrivileges::from_account(AccessLevel::Developer),
                    in_world: true,
                },
                context,
                token: 42,
                definitions: &[],
                equipment: vec![],
            },
            |target| {
                assert_eq!(target, StaffTarget::SelfActor);
                Ok(context.actor)
            },
        )
        .unwrap();
        let PreparedStaffGame::Simulation(command) = prepared else {
            panic!("myloc must reach the simulation owner")
        };
        assert_eq!(
            command.action,
            StaffAction::Inspect {
                context,
                target: context.actor,
                kind: StaffInspection::SelfPosition,
                sudo: false,
            }
        );
    }

    #[test]
    fn developer_gps_dispatches_distinct_authoritative_self_inspection() {
        let context = ActionContext {
            actor: EntityId(0x5000_0001),
            account: AccountId(7),
            session: SessionId(9),
            sequence: 11,
        };
        let prepared = prepare_staff_game_command(
            StaffGameInput {
                line: "@gps",
                principal: StaffPrincipal {
                    account_access: AccessLevel::Developer,
                    character: CharacterPrivileges::from_account(AccessLevel::Developer),
                    in_world: true,
                },
                context,
                token: 42,
                definitions: &[],
                equipment: vec![],
            },
            |target| {
                assert_eq!(target, StaffTarget::SelfActor);
                Ok(context.actor)
            },
        )
        .unwrap();
        let PreparedStaffGame::Simulation(command) = prepared else {
            panic!("gps must reach the simulation owner")
        };
        assert_eq!(
            command.action,
            StaffAction::Inspect {
                context,
                target: context.actor,
                kind: StaffInspection::Gps,
                sudo: false,
            }
        );
    }

    #[test]
    fn developer_whoami_dispatches_the_distinct_private_guid_inspection() {
        let context = ActionContext {
            actor: EntityId(0x5000_0001),
            account: AccountId(7),
            session: SessionId(9),
            sequence: 11,
        };
        let prepared = prepare_staff_game_command(
            StaffGameInput {
                line: "@whoami",
                principal: StaffPrincipal {
                    account_access: AccessLevel::Developer,
                    character: CharacterPrivileges::from_account(AccessLevel::Developer),
                    in_world: true,
                },
                context,
                token: 43,
                definitions: &[],
                equipment: vec![],
            },
            |target| {
                assert_eq!(target, StaffTarget::SelfActor);
                Ok(context.actor)
            },
        )
        .unwrap();
        let PreparedStaffGame::Simulation(command) = prepared else {
            panic!("whoami must reach simulation")
        };
        assert_eq!(
            command.action,
            StaffAction::Inspect {
                context,
                target: context.actor,
                kind: StaffInspection::WhoAmI,
                sudo: false,
            }
        );
    }

    #[test]
    fn envoy_regen_becomes_one_authoritative_staff_command() {
        let context = ActionContext {
            actor: EntityId(0x5000_0001),
            account: AccountId(7),
            session: SessionId(9),
            sequence: 11,
        };
        let prepared = prepare_staff_game_command(
            StaffGameInput {
                line: "@regen",
                principal: StaffPrincipal {
                    account_access: AccessLevel::Envoy,
                    character: CharacterPrivileges::from_account(AccessLevel::Envoy),
                    in_world: true,
                },
                context,
                token: 42,
                definitions: &[],
                equipment: vec![],
            },
            |target| {
                assert_eq!(target, StaffTarget::SelectedGenerator);
                Ok(EntityId(0x7000_0002))
            },
        )
        .unwrap();
        let PreparedStaffGame::Simulation(command) = prepared else {
            panic!("regen must reach the simulation owner")
        };
        assert_eq!(command.token, 42);
        assert_eq!(
            command.action,
            StaffAction::Regenerate {
                context,
                target: EntityId(0x7000_0002),
                sudo: false,
            }
        );
    }

    #[test]
    fn sentinel_boot_reaches_the_staff_runtime_owner_without_simulation_mutation() {
        let context = ActionContext {
            actor: EntityId(0x5000_0001),
            account: AccountId(7),
            session: SessionId(9),
            sequence: 11,
        };
        let prepared = prepare_staff_game_command(
            StaffGameInput {
                line: "@boot iid 0x50000002, repeated abuse",
                principal: StaffPrincipal {
                    account_access: AccessLevel::Sentinel,
                    character: CharacterPrivileges::from_account(AccessLevel::Sentinel),
                    in_world: true,
                },
                context,
                token: 42,
                definitions: &[],
                equipment: vec![],
            },
            |_| panic!("boot resolves online sessions in the staff runtime"),
        )
        .unwrap();
        let PreparedStaffGame::Other(command) = prepared else {
            panic!("boot must retain its runtime owner")
        };
        assert_eq!(command.spec.name, "boot");
        assert!(matches!(
            bace_admin::prepare_staff_operation(&command).unwrap(),
            Some(StaffOperation::Boot { .. })
        ));
    }
}
