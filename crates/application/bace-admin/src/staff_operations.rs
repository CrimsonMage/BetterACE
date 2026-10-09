//! Typed owner dispatch after catalog authorization. Conversion is not execution;
//! durable owner receipts, asset preparation and target resolution remain required.
use crate::{AuthorizedCommand, CommandError};
use bace_auth::AccessLevel;
use bace_gameplay_api::staff::StaffDestination;
#[derive(Clone)]
pub struct SecretArgument(String);
impl SecretArgument {
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for SecretArgument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StaffTarget {
    SelfActor,
    Selected,
    /// ACE HandleRegen: HealthQueryTarget, then ManaQueryTarget, then
    /// CurrentAppraisalTarget, all resolved from the current owner snapshot.
    SelectedGenerator,
    SelectedOrSelf,
    Named(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inspection {
    Identity,
    WhoAmI,
    SelfPosition,
    Gps,
    Position,
    Enchantments,
    Vitals,
}
#[derive(Clone, Debug)]
pub enum AccountOperation {
    Create {
        name: String,
        password: SecretArgument,
        access: Option<AccessLevel>,
    },
    Get {
        name: String,
    },
    SetAccess {
        name: String,
        access: AccessLevel,
    },
    SetPassword {
        name: String,
        password: SecretArgument,
    },
    ChangeOwnPassword {
        old: SecretArgument,
        new: SecretArgument,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum TeleportOperation {
    ToPlayer(String),
    BringPlayer(String),
    ReturnPlayer(String),
    Location(StaffDestination),
    Offset([f32; 3]),
    Distance(f32),
    PointOfInterest(String),
    Dungeon(String),
}
#[derive(Clone, Debug)]
pub enum StaffOperation {
    Time,
    ListPlayers {
        access: Option<String>,
    },
    Ban {
        name: String,
        duration: [String; 3],
        reason: Option<String>,
    },
    Unban {
        name: String,
    },
    BanList,
    Boot {
        selector: BootSelector,
        name: String,
        reason: Option<String>,
    },
    GrantExperience {
        target: StaffTarget,
        amount: u64,
    },
    Help {
        filter: Option<String>,
    },
    Inspect {
        target: StaffTarget,
        kind: Inspection,
    },
    Heal {
        target: StaffTarget,
    },
    Teleport(TeleportOperation),
    Account(AccountOperation),
    Gag {
        name: String,
        seconds: Option<u32>,
    },
    Broadcast {
        text: String,
        emote: bool,
        local: bool,
    },
    CastSpell {
        spell: u32,
        target: StaffTarget,
    },
    Run(bace_gameplay_api::staff::StaffRunMode),
    Buff {
        target: StaffTarget,
        maximum_level: u8,
        fellowship: bool,
    },
    RemoveSpell {
        spell: String,
    },
    AddSpell {
        spell: String,
    },
    Regenerate {
        target: StaffTarget,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootSelector {
    Character,
    Account,
    Instance,
    Invalid,
}
fn argument(args: &[String], index: usize) -> Result<&str, CommandError> {
    args.get(index)
        .map(String::as_str)
        .ok_or(CommandError::InvalidParameterCount)
}
fn float(text: &str) -> Result<f32, CommandError> {
    let value: f32 = text
        .trim_matches([' ', '[', ']'])
        .parse()
        .map_err(|_| CommandError::InvalidParameterCount)?;
    if !value.is_finite() {
        return Err(CommandError::InvalidParameterCount);
    }
    Ok(value)
}
fn access(text: &str) -> Result<AccessLevel, CommandError> {
    let value = match text.to_ascii_lowercase().as_str() {
        "player" => 0,
        "advocate" => 1,
        "sentinel" => 2,
        "envoy" => 3,
        "developer" => 4,
        "admin" => 5,
        _ => text
            .parse()
            .map_err(|_| CommandError::InvalidParameterCount)?,
    };
    AccessLevel::try_from(value).map_err(|_| CommandError::InvalidParameterCount)
}
/// None means this command needs its separately registered owner handler. It is
/// never a successful no-op or a claim that the pinned handler is unsupported.
pub fn prepare_staff_operation(
    command: &AuthorizedCommand,
) -> Result<Option<StaffOperation>, CommandError> {
    let args = if command.spec.include_raw {
        command
            .arguments
            .get(1..)
            .ok_or(CommandError::InvalidParameterCount)?
    } else {
        &command.arguments[..]
    };
    let first = || argument(args, 0).map(str::to_owned);
    let result = match command.spec.name {
        "time" => StaffOperation::Time,
        "listplayers" => StaffOperation::ListPlayers {
            // Pinned handler reads only parameters[0] and validates after
            // command authorization, including numeric enum values.
            access: args.first().cloned(),
        },
        "ban" => StaffOperation::Ban {
            name: first()?,
            // ACE resolves the account before parsing duration, so preserve
            // each source argument for the owning runtime response path.
            duration: [
                argument(args, 1)?.into(),
                argument(args, 2)?.into(),
                argument(args, 3)?.into(),
            ],
            reason: (!args[4..].join(" ").trim().is_empty()).then(|| args[4..].join(" ")),
        },
        "unban" => StaffOperation::Unban { name: first()? },
        "banlist" => StaffOperation::BanList,
        "boot" => {
            let selector = match argument(args, 0)?.to_ascii_lowercase().as_str() {
                "char" => BootSelector::Character,
                "account" => BootSelector::Account,
                "iid" => BootSelector::Instance,
                _ => BootSelector::Invalid,
            };
            // ACE joins everything after the selector, then separates the
            // optional reason at the first comma.
            let joined = args[1..].join(" ");
            let (name, reason) = match joined.split_once(',') {
                Some((name, reason)) => (name.trim(), Some(reason.trim().to_owned())),
                None => (joined.trim(), None),
            };
            StaffOperation::Boot {
                selector,
                name: name.to_owned(),
                reason,
            }
        }
        "grantxp" => {
            let (target, amount) = match args {
                [amount] => (StaffTarget::SelfActor, amount),
                [name, amount, ..] => (StaffTarget::Named(name.clone()), amount),
                _ => return Err(CommandError::InvalidParameterCount),
            };
            let amount: u64 = amount
                .parse()
                .map_err(|_| CommandError::InvalidParameterCount)?;
            if amount == 0 || amount > i64::MAX as u64 {
                return Err(CommandError::InvalidParameterCount);
            }
            StaffOperation::GrantExperience { target, amount }
        }
        "acecommands" | "acehelp" => StaffOperation::Help {
            filter: args.first().cloned(),
        },
        "myiid" => StaffOperation::Inspect {
            target: StaffTarget::SelfActor,
            kind: Inspection::Identity,
        },
        "whoami" => StaffOperation::Inspect {
            target: StaffTarget::SelfActor,
            kind: Inspection::WhoAmI,
        },
        "myloc" => StaffOperation::Inspect {
            target: StaffTarget::SelfActor,
            kind: Inspection::SelfPosition,
        },
        "gps" => StaffOperation::Inspect {
            target: StaffTarget::SelfActor,
            kind: Inspection::Gps,
        },
        "targetloc" => {
            // ACE accepts an optional GUID and resolves it from the current
            // landblock, then its global object manager. That route needs a
            // separate owner; never silently inspect the selected object.
            if !args.is_empty() {
                return Err(CommandError::UnsupportedVariant);
            }
            StaffOperation::Inspect {
                target: StaffTarget::Selected,
                kind: Inspection::Position,
            }
        }
        "getenchantments" => StaffOperation::Inspect {
            target: StaffTarget::Selected,
            kind: Inspection::Enchantments,
        },
        "heal" => StaffOperation::Heal {
            target: StaffTarget::SelectedOrSelf,
        },
        "teleto" => StaffOperation::Teleport(TeleportOperation::ToPlayer(args.join(" "))),
        "teletome" => StaffOperation::Teleport(TeleportOperation::BringPlayer(args.join(" "))),
        "telereturn" => StaffOperation::Teleport(TeleportOperation::ReturnPlayer(args.join(" "))),
        "telepoi" => StaffOperation::Teleport(TeleportOperation::PointOfInterest(args.join(" "))),
        "teledungeon" => StaffOperation::Teleport(TeleportOperation::Dungeon(args.join(" "))),
        "telexyz" => StaffOperation::Teleport(TeleportOperation::Location(StaffDestination {
            cell: argument(args, 0)?
                .parse()
                .map_err(|_| CommandError::InvalidParameterCount)?,
            origin: [
                float(argument(args, 1)?)?,
                float(argument(args, 2)?)?,
                float(argument(args, 3)?)?,
            ],
            rotation: [
                float(argument(args, 7)?)?,
                float(argument(args, 4)?)?,
                float(argument(args, 5)?)?,
                float(argument(args, 6)?)?,
            ],
        })),
        "teledist" => {
            StaffOperation::Teleport(TeleportOperation::Distance(float(argument(args, 0)?)?))
        }
        "teleloc" => {
            let cell = u32::from_str_radix(argument(args, 0)?.trim_start_matches("0x"), 16)
                .map_err(|_| CommandError::InvalidParameterCount)?;
            let mut rotation = [1., 0., 0., 0.];
            if args.len() >= 8 {
                for (index, value) in rotation.iter_mut().enumerate() {
                    *value = float(argument(args, index + 4)?)?;
                }
            }
            StaffOperation::Teleport(TeleportOperation::Location(StaffDestination {
                cell,
                origin: [
                    float(argument(args, 1)?)?,
                    float(argument(args, 2)?)?,
                    float(argument(args, 3)?)?,
                ],
                rotation,
            }))
        }
        "accountcreate" => StaffOperation::Account(AccountOperation::Create {
            name: first()?,
            password: SecretArgument(argument(args, 1)?.into()),
            access: args.get(2).map(|s| access(s)).transpose()?,
        }),
        "accountget" => StaffOperation::Account(AccountOperation::Get { name: first()? }),
        "set-accountaccess" => StaffOperation::Account(AccountOperation::SetAccess {
            name: first()?,
            access: args.get(1).map_or(Ok(AccessLevel::Player), |s| access(s))?,
        }),
        "set-accountpassword" => StaffOperation::Account(AccountOperation::SetPassword {
            name: first()?,
            password: SecretArgument(argument(args, 1)?.into()),
        }),
        "passwd" => StaffOperation::Account(AccountOperation::ChangeOwnPassword {
            old: SecretArgument(first()?),
            new: SecretArgument(argument(args, 1)?.into()),
        }),
        "gag" => StaffOperation::Gag {
            name: args.join(" "),
            seconds: Some(300),
        },
        "ungag" => StaffOperation::Gag {
            name: args.join(" "),
            seconds: None,
        },
        "gamecast" | "gamecastlocal" => StaffOperation::Broadcast {
            text: args.join(" "),
            emote: false,
            local: command.spec.name == "gamecastlocal",
        },
        "gamecastemote" | "gamecastlocalemote" | "we" => StaffOperation::Broadcast {
            text: args.join(" "),
            emote: true,
            local: command.spec.name == "gamecastlocalemote",
        },
        "castspell" => StaffOperation::CastSpell {
            spell: argument(args, 0)?
                .parse()
                .map_err(|_| CommandError::InvalidParameterCount)?,
            target: StaffTarget::Selected,
        },
        "run" => StaffOperation::Run(match args.first().map(String::as_str).unwrap_or("toggle") {
            "on" => bace_gameplay_api::staff::StaffRunMode::On,
            "off" => bace_gameplay_api::staff::StaffRunMode::Off,
            "toggle" => bace_gameplay_api::staff::StaffRunMode::Toggle,
            "check" => bace_gameplay_api::staff::StaffRunMode::Check,
            _ => return Err(CommandError::InvalidParameterCount),
        }),
        "buff" | "fellowbuff" => StaffOperation::Buff {
            target: args.first().map_or(StaffTarget::SelfActor, |name| {
                StaffTarget::Named(name.clone())
            }),
            maximum_level: if command.spec.name == "fellowbuff" {
                8
            } else {
                args.get(1)
                    .map_or(Ok(8u64), |level| level.parse::<u64>())
                    .map_err(|_| CommandError::InvalidParameterCount)?
                    .clamp(1, 8) as u8
            },
            fellowship: command.spec.name == "fellowbuff",
        },
        "removespell" => StaffOperation::RemoveSpell {
            spell: argument(args, 0)?.to_owned(),
        },
        "addspell" => StaffOperation::AddSpell {
            spell: argument(args, 0)?.to_owned(),
        },
        "regen" => StaffOperation::Regenerate {
            target: StaffTarget::SelectedGenerator,
        },
        _ => return Ok(None),
    };
    Ok(Some(result))
}
