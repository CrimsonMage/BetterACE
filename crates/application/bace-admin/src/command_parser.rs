//! Bounded ACE command parsing and authorization, separate from execution.
use crate::command_catalog::{CommandSpec, command};
use bace_auth::StaffPrincipal;
#[derive(Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub name: String,
    pub arguments: Vec<String>,
    pub raw_arguments: String,
}
#[derive(Clone)]
pub struct AuthorizedCommand {
    pub spec: &'static CommandSpec,
    pub arguments: Vec<String>,
    pub raw_arguments: String,
    pub sudo: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandError {
    InvalidCommand,
    Limit,
    UnterminatedQuote,
    NotAuthorized,
    ConsoleOnly,
    NotInWorld,
    InvalidParameterCount,
    SourceUnimplemented,
    Incompatible(&'static str),
}
/// ACE splits on ASCII spaces, joins quoted parameters and removes quote marks.
/// Malformed unmatched quotes are rejected, instead of ACE's duplicate tail.
pub fn parse_command(line: &str) -> Result<ParsedCommand, CommandError> {
    if line.len() > 8192 || line.chars().any(|c| c.is_control() && c != '\t') {
        return Err(CommandError::Limit);
    }
    let mut words = line.split(' ').filter(|p| !p.is_empty());
    let first = words.next().ok_or(CommandError::InvalidCommand)?;
    let name = first.strip_prefix(['@', '/']).unwrap_or(first);
    if name.is_empty() || name.len() > 128 {
        return Err(CommandError::InvalidCommand);
    }
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for word in words {
        if arguments.len() >= 64 {
            return Err(CommandError::Limit);
        }
        if quoted {
            current.push(' ');
            current.push_str(&word.replace('"', ""));
            if word.ends_with('"') {
                arguments.push(std::mem::take(&mut current));
                quoted = false;
            }
        } else if word.starts_with('"') && !word.ends_with('"') {
            current = word.replace('"', "");
            quoted = true;
        } else {
            arguments.push(word.replace('"', ""));
        }
    }
    if quoted {
        return Err(CommandError::UnterminatedQuote);
    }
    let raw_arguments = line
        .trim_start_matches(' ')
        .strip_prefix(first)
        .ok_or(CommandError::InvalidCommand)?
        .trim_start()
        .to_owned();
    Ok(ParsedCommand {
        name: name.to_owned(),
        arguments,
        raw_arguments,
    })
}
/// A missing principal means a separately authenticated host operator, never an
/// anonymous network caller. Game session callers must always supply a principal.
pub fn authorize_command(
    mut parsed: ParsedCommand,
    principal: Option<StaffPrincipal>,
) -> Result<AuthorizedCommand, CommandError> {
    let sudo = parsed.name.eq_ignore_ascii_case("sudo");
    if sudo {
        if principal.is_none() || parsed.arguments.is_empty() {
            return Err(CommandError::InvalidCommand);
        }
        parsed.name = parsed.arguments.remove(0);
        // Source raw argument inclusion follows command stripping. Never expose
        // account credentials or raw command lines in Debug diagnostics.
        parsed.raw_arguments = parsed
            .raw_arguments
            .strip_prefix(&parsed.name)
            .unwrap_or(&parsed.raw_arguments)
            .trim_start()
            .to_owned();
    }
    let spec = command(&parsed.name).ok_or(CommandError::InvalidCommand)?;
    if let Some(principal) = principal {
        if spec.console_only {
            return Err(CommandError::ConsoleOnly);
        }
        if !principal.allows(spec.access, sudo) {
            return Err(CommandError::NotAuthorized);
        }
    }
    if spec.min_args >= 0 && parsed.arguments.len() < (spec.min_args as usize) {
        return Err(CommandError::InvalidParameterCount);
    }
    if spec.requires_world && principal.is_none_or(|p| !p.in_world) {
        return Err(CommandError::NotInWorld);
    }
    if spec.source_stub {
        return Err(CommandError::SourceUnimplemented);
    }
    if let crate::CommandCompatibility::Incompatible(reason) = crate::command_compatibility(spec) {
        return Err(CommandError::Incompatible(reason));
    }
    if spec.include_raw {
        parsed.arguments.insert(0, parsed.raw_arguments.clone());
    }
    Ok(AuthorizedCommand {
        spec,
        arguments: parsed.arguments,
        raw_arguments: parsed.raw_arguments,
        sudo,
    })
}

impl std::fmt::Debug for ParsedCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedCommand")
            .field("name", &self.name)
            .field("argument_count", &self.arguments.len())
            .finish_non_exhaustive()
    }
}
impl std::fmt::Debug for AuthorizedCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizedCommand")
            .field("name", &self.spec.name)
            .field("argument_count", &self.arguments.len())
            .field("sudo", &self.sudo)
            .finish_non_exhaustive()
    }
}
