//! Frozen command metadata from official ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b.
use bace_auth::AccessLevel;
mod admin_a_m;
mod admin_n_z;
mod advocate;
mod developer_a_f;
mod developer_g_m;
mod developer_n_s;
mod developer_t_z;
mod envoy;
mod player;
mod sentinel;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: &'static str,
    pub access: AccessLevel,
    pub min_args: i32,
    pub console_only: bool,
    pub requires_world: bool,
    pub include_raw: bool,
    pub source_stub: bool,
    pub handler: &'static str,
    pub source: &'static str,
    pub description: &'static str,
    pub usage: &'static str,
}
pub fn commands() -> impl Iterator<Item = &'static CommandSpec> {
    admin_a_m::COMMANDS
        .iter()
        .chain(admin_n_z::COMMANDS)
        .chain(advocate::COMMANDS)
        .chain(developer_a_f::COMMANDS)
        .chain(developer_g_m::COMMANDS)
        .chain(developer_n_s::COMMANDS)
        .chain(developer_t_z::COMMANDS)
        .chain(envoy::COMMANDS)
        .chain(player::COMMANDS)
        .chain(sentinel::COMMANDS)
}
pub fn command(name: &str) -> Option<&'static CommandSpec> {
    commands().find(|spec| spec.name.eq_ignore_ascii_case(name))
}
