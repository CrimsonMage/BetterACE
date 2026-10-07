# bace-import

Implemented: `import_weenie_json` automatically detects official Lifestoned/GDLE (`wcid`) and `ACE.Entity.Models.Weenie` (`WeenieClassId`) JSON shapes. It accepts numeric property IDs and pinned upstream enum names, preserves unknown numeric IDs and ordered data, validates widths, and rejects duplicate/unknown fields rather than dropping them. JSON exists only at this legacy tooling boundary. Database-ORM and unknown JSON dialects are not implemented and fail explicitly.

Field and enum-name mappings derive from official ACE at `47edade3bd3f6044b676d4eb877c4965c7eda62b` (AGPL-3.0-only). A null `PropertiesEmote.Object` is a navigation backreference; embedded objects are rejected. Empty/null property collections have the same upstream meaning and normalize to empty collections.

Lifestoned support maps every serialized gameplay section in the pinned `ACE.Adapter.GDLE.Models.LSDWeenie`: scalar stats, attributes/vitals, body, books/pages, create lists, skills, emotes/actions, spells, positions and generators. Authoring metadata/changelog, generator slots and emote group keys remain typed native fields. Class-name derivation uses pinned upstream enum precedence/fallback. The source declares some fields wider than ACE storage (for example HeroXP64); unrepresentable values fail rather than wrap. Boolean values must be 0/1. Ancient enum-shift conversion is not enabled implicitly.

Fixture `tests/fixtures/content/lifestoned-weenie.json` is synthetic, uses the upstream serialized names, and is exercised through import -> native TOML -> binary -> native TOML. It contains no player data. This establishes conversion coverage, not complete stock-client/world parity.

SQL status: **implemented weenie extraction**, not full-world conversion. `MariaDbStaging` initializes a new temporary datadir, runs its own private-socket server with TCP disabled, loads the pinned world schema, snapshots/hashes the unchanged input SQL and sends it to MariaDB's real SQL parser. No existing socket, database URL or production credentials are accepted. The importer owns and terminates its child and removes its private files on success/failure. `MariaDbBinaries` configures executable locations for a private installation; the game runtime does not need MariaDB.

All 24 weenie/property tables have fixed checked mappings. Extracted counts must match source counts; unfamiliar columns, populated unsupported tables/databases, failed SQL, duplicate semantics and SQL warnings reject the whole conversion. Emote actions and pages use explicit database ordering and preserve original sequence IDs. A narrow staging migration corrects the old WorldBase.sql signed `motion` column to unsigned, matching the pinned current `WeeniePropertiesEmoteAction.Motion` model. Sources are copied into a private snapshot capped at 512 MiB; per-table tool interchange is capped at 256 MiB. Subprocesses have explicit deadlines (initialization 30 seconds, query 60 seconds, load 120 seconds), capped output, and isolated temporary directories. Timeout cleanup terminates the owned process group, including bootstrap children. Oversized, timed-out or lossy inputs fail explicitly.

A concrete integration suite imports an unchanged official Arrow SQL file and synthetic complex SQL, exercises NULL/false, LAST_INSERT_ID, Unicode/quotes/semicolons, uint motion and i64 limits, and rejects malformed, lossy, unsupported and schema-changing input. Run explicitly with:

```sh
BACE_MARIADB_BASEDIR=/absolute/private/mariadb/usr cargo test -p bace-import --test mariadb -- --ignored
```

The prerequisite test is ignored in the normal dependency-free suite; an ignored test is not a passed MariaDB integration check. `probe_mariadb` diagnoses a PATH client only; configured private binaries do not need to appear on PATH. Non-weenie full-world import remains unsupported and errors rather than discarding those tables.

Never execute the upstream dump against production PostgreSQL or an existing ACE database: its database recreation statements are destructive. Do not regex-parse SQL. Non-weenie world table conversion remains unsupported and must not be represented as a completed full-world import.

Legacy exports: `export_weenie_json` emits the pinned ACE.Entity.Models.Weenie
JSON shape; `export_weenie_sql` emits INSERT-only SQL for the pinned world schema.
They validate native data first and reject authoring fields without legacy
representations. Content Studio retains the complete native TOML in a companion
and explicitly reports metadata stored only there before calling these strict
exporters. SQL destination row IDs are regenerated; absent page/action sequence
IDs are filled from array order and absent timestamps use destination defaults.
Explicit sequence IDs inconsistent with array order reject export. Strings use
UTF-8 hex expressions to avoid SQL quoting/mode ambiguities. Exports are files
only, never SQL execution against a live server. Existing IDs cause INSERT errors.

Field/column provenance is the same official ACE pin and WorldBase.sql inventory
as import. `tests/export.rs` checks the official JSON shape and exact i64 values;
its ignored prerequisite integration test exports the complex SQL fixture and
reimports with real MariaDB, comparing all authored fields (destination storage
row IDs are explicitly normalized). Run with `BACE_MARIADB_BASEDIR` and
`cargo test -p bace-import --test export -- --ignored`. This is not a claim of
exhaustive legacy exporter parity or support for all SQL schema variants.
