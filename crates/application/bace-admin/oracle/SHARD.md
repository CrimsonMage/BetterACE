# Shard command oracle

Authority: official ACEmulator/ACE commit
`47edade3bd3f6044b676d4eb877c4965c7eda62b`, AGPL-3.0-only.
Upstream attribution and source files remain unchanged.

Run `python3 crates/application/bace-admin/oracle/run_shard.py [dotnet]`.
The harness compiles the entire original `AdminShardCommands.cs`, attribute/flag
and access declarations, and `DateTimeExtensions.cs`. It extracts unchanged
`ServerManager.SetShutdownInterval`, `CancelShutdown`,
`NotifyPlayersOfPendingShutdown`, `WorldManager.Open/Close` and
`PlayerManager.BootAllPlayers`. Source hashes accompany the output at
`bace-runtime/tests/fixtures/shard_commands.json`.

Stubs collect actual log, audit, direct reply and broadcast messages; world boot
runs against one session at every source access level. The handler's static
`DateTime` lookup is supplied by a namespace-local fixed clock. The notification
method receives a fixed aliased value type with genuine .NET TimeSpan arithmetic.
Thread creation is stubbed: cancellation worker effects are separately asserted
by Rust tests after comparing the handler's original output prefix. Persistence
and shutdown hardening are tested as BetterACE contracts, not claimed as source
implementation details. These vectors do not establish client playability.
