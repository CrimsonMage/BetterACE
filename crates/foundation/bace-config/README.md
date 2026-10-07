# bace-config

Status: implemented.

TOML configuration loading and validation.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.


Optional TOML `[accounts]`, `[network]` and `[dat_distribution]` sections preserve
existing config compatibility. `accounts.allow_auto_creation` defaults true;
`dat_distribution.enabled` defaults false and enabling it requires `dat_directory`.
Network settings bound worker/queue/admission budgets and authentication/session
timeouts. Enabling a setting does not bypass runtime readiness or asset validation.
