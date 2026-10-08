//! Pinned ACE AdminShardCommands input preparation. Runtime rechecks authority.
use crate::{AuthorizedCommand, CommandError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShardOperation {
    CancelShutdown,
    SetShutdownInterval(Option<u32>),
    StopNow { message: String },
    Shutdown { message: String },
    World { open: Option<bool>, boot: bool },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedShardCommand {
    operation: ShardOperation,
    sudo: bool,
}
impl PreparedShardCommand {
    pub fn operation(&self) -> &ShardOperation {
        &self.operation
    }
    pub fn sudo(&self) -> bool {
        self.sudo
    }
}
/// Unknown catalog handlers remain with their owning dispatcher. Invalid interval
/// text is a supported request: the source handler emits its usage response.
pub fn prepare_shard_command(
    command: &AuthorizedCommand,
) -> Result<Option<PreparedShardCommand>, CommandError> {
    if command.arguments.len() > 64
        || command.arguments.iter().map(String::len).sum::<usize>() > 8192
    {
        return Err(CommandError::Limit);
    }
    let args = &command.arguments;
    let operation = match command.spec.name {
        "cancel-shutdown" => ShardOperation::CancelShutdown,
        "set-shutdown-interval" => ShardOperation::SetShutdownInterval(parse_interval(
            args.first().ok_or(CommandError::InvalidParameterCount)?,
        )),
        "stop-now" => ShardOperation::StopNow {
            message: args.join(" "),
        },
        "shutdown" => ShardOperation::Shutdown {
            message: args.join(" "),
        },
        "world" => ShardOperation::World {
            open: args.first().and_then(|a| {
                if a.eq_ignore_ascii_case("open") {
                    Some(true)
                } else if a.eq_ignore_ascii_case("close") {
                    Some(false)
                } else {
                    None
                }
            }),
            boot: args.get(1).is_some_and(|a| a.eq_ignore_ascii_case("boot")),
        },
        _ => return Ok(None),
    };
    Ok(Some(PreparedShardCommand {
        operation,
        sudo: command.sudo,
    }))
}
/// C# Substring counts UTF-16 units; uint.TryParse accepts an optional sign and
/// ASCII digits, including negative zero. Numeric whitespace is ASCII only.
fn parse_interval(input: &str) -> Option<u32> {
    let prefix: Vec<u16> = input.encode_utf16().take(5).collect();
    let prefix = String::from_utf16(&prefix).ok()?;
    let text = prefix
        .trim_end_matches('\0')
        .trim_matches(|c: char| matches!(c, '\t'..='\r' | ' '));
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'+') => (false, &text[1..]),
        Some(b'-') => (true, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value = digits.parse::<u32>().ok()?;
    (!negative || value == 0).then_some(value)
}
