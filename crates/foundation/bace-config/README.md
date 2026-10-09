# bace-config

Status: implemented.

TOML configuration loading and validation.

`ServerConfig::load_or_create_default(path)` creates a commented, valid
`server.toml` on first run and returns whether it created the file. It publishes
the complete template without replacing an existing file. The generated file
leaves `dat_directory`, `pack_directory`, and `random_key_file` visibly
unconfigured; gameplay readiness remains closed until the operator provisions
those paths and the PostgreSQL URL environment variable.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.


Optional TOML `[accounts]`, `[network]` and `[dat_distribution]` sections preserve
existing config compatibility. `accounts.allow_auto_creation` defaults true;
`dat_distribution.enabled` defaults false and enabling it requires `dat_directory`.
Network settings bound worker/queue/admission budgets and authentication/session
timeouts. Enabling a setting does not bypass runtime readiness or asset validation.

`random_key_file` names the provisioned private gameplay RNG key. The default is
unset; runtime refuses random gameplay admission until the file is loaded and its
fingerprint is bound to the database. Provisioning is an explicit tooling action.

`treasure_table_set_id` defaults to 1 (pinned ACE) and selects an accepted
namespace-52 `.bace` table set for the game child at startup. A missing selected
record blocks startup; changing the selection requires a child restart.

`content_inbox_directory` defaults to `state/content-inbox` and names native
TOML staged for authenticated host review. Files there are never imported on
startup or by a filesystem watcher. `pack_directory` and a working PostgreSQL
URL are required before a review can compare against the accepted generation.

`[world]` settings are restart-only immutable policy inputs. Default zone sets are
the pinned ACE 37 no-relog, 19 no-death-drop and 12 no-kill-XP landblocks; explicit
empty lists replace defaults. `world.example.toml` documents six startup preload
entries, including Hebian-To enabled and Holtburg disabled. Preload requests cold
preparation once; permanent residency additionally prevents inactivity unload.
`ace_world_zones` compares all defaults with the compiled-original ACE fixture.
`world.death.creatures_drop_createlist_wield` is an explicit restart-only ACE
treasure-selection option. It defaults to false; both original-oracle branches
can be selected without inferring the value from a live item placement.
`[chat_api]` is separately disabled by default and accepts only a loopback listener;
bot credentials remain in a separate private file.

`[chat_policy]` carries restart-only ACE channel gates, separately from
`[chat_api]` bot authorization. Defaults match `PropertyManager`: all disable,
echo and age flags false, `inform_reject=true`, numeric age/level thresholds zero.
Negative numeric thresholds and unknown keys are rejected.
