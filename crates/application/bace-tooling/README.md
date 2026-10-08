# bace-tooling

Status: foundation.

Runnable conversion, content publication/status, migration, fresh account creation and DAT inspection workflows. Complete-world import and aggregate pack compilation are implemented; legacy player migration remains unsupported.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.


`local-database --directory .local/postgres` initializes or restarts a private
Unix-socket PostgreSQL and applies migrations. It prints the environment assignment;
no game account is auto-promoted and no system database is changed.

`world-build --input complete.sql --output-directory NEW_DIR` uses the configured
private MariaDB staging backend, preserves the complete world tables and writes
one aggregate `.bace` base, a binary manifest and a small TOML import report.
`world-activate --manifest FILE` checks payloads and accepts the initial generation
in PostgreSQL. It does not import world rows into PostgreSQL or spawn a world.
`--reindex` permits a parent-CAS layout replacement only after every logical source
record compares identical; changed content still requires the publication journal.

Native loot workflows: `loot-build` compiles many TOML tables/profiles into one
aggregate supplement; `loot-sample` produces bounded deterministic synthetic
frequency counts without player state; `loot-publish` queues binary namespace
46/47 candidates in the durable publication journal. Publication is a separate
validated runtime operation, not implied by successful queueing. `rng-init`
provisions a private versioned key (Unix helper) and binds its immutable fingerprint
in PostgreSQL. No uncertain rare probability is enabled. See
`docs/native-loot-and-rares.md` for examples and remaining integration boundaries.
