# bace-import

Implemented: `import_weenie_json` automatically detects official Lifestoned/GDLE (`wcid`) and `ACE.Entity.Models.Weenie` (`WeenieClassId`) JSON shapes. It accepts numeric property IDs and pinned upstream enum names, preserves unknown numeric IDs and ordered data, validates widths, and rejects duplicate/unknown fields rather than dropping them. JSON exists only at this legacy tooling boundary. Database-ORM and unknown JSON dialects are not implemented and fail explicitly.

Field and enum-name mappings derive from official ACE at `47edade3bd3f6044b676d4eb877c4965c7eda62b` (AGPL-3.0-only). A null `PropertiesEmote.Object` is a navigation backreference; embedded objects are rejected. Empty/null property collections have the same upstream meaning and normalize to empty collections.

Lifestoned support maps every serialized gameplay section in the pinned `ACE.Adapter.GDLE.Models.LSDWeenie`: scalar stats, attributes/vitals, body, books/pages, create lists, skills, emotes/actions, spells, positions and generators. Authoring metadata/changelog, generator slots and emote group keys remain typed native fields. Class-name derivation uses pinned upstream enum precedence/fallback. The source declares some fields wider than ACE storage (for example HeroXP64); unrepresentable values fail rather than wrap. Boolean values must be 0/1. Ancient enum-shift conversion is not enabled implicitly.

Fixture `tests/fixtures/content/lifestoned-weenie.json` is synthetic, uses the upstream serialized names, and is exercised through import -> native TOML -> binary -> native TOML. It contains no player data. This establishes conversion coverage, not complete stock-client/world parity.

SQL status: **implemented weenie extraction and complete-world conversion** (separate APIs; see below). `MariaDbStaging` initializes a new temporary datadir, runs its own private-socket server with TCP disabled, loads the pinned world schema, snapshots/hashes the unchanged input SQL and sends it to MariaDB's real SQL parser. No existing socket, database URL or production credentials are accepted. The importer owns and terminates its child and removes its private files on success/failure. `MariaDbBinaries` configures executable locations for a private installation; the game runtime does not need MariaDB.

All 24 weenie/property tables have fixed checked mappings. Extracted counts must match source counts; unfamiliar columns, populated unsupported tables/databases, failed SQL, duplicate semantics and SQL warnings reject the whole conversion. Emote actions and pages use explicit database ordering and preserve original sequence IDs. A narrow staging migration corrects the old WorldBase.sql signed `motion` column to unsigned, matching the pinned current `WeeniePropertiesEmoteAction.Motion` model. Sources are copied into a private snapshot capped at 512 MiB; per-table tool interchange is capped at 256 MiB. Subprocesses have explicit deadlines (initialization 30 seconds, query 60 seconds, load 120 seconds), capped output, and isolated temporary directories. Timeout cleanup terminates the owned process group, including bootstrap children. Oversized, timed-out or lossy inputs fail explicitly.

A concrete integration suite imports an unchanged official Arrow SQL file and synthetic complex SQL, exercises NULL/false, LAST_INSERT_ID, Unicode/quotes/semicolons, uint motion and i64 limits, and rejects malformed, lossy, unsupported and schema-changing input. Run explicitly with:

```sh
BACE_MARIADB_BASEDIR=/absolute/private/mariadb/usr cargo test -p bace-import --test mariadb -- --ignored
```

The prerequisite test is ignored in the normal dependency-free suite; an ignored test is not a passed MariaDB integration check. `probe_mariadb` diagnoses a PATH client only; configured private binaries do not need to appear on PATH. The weenie-only API continues rejecting non-weenie rows; use the complete-world API below to preserve them.

Never execute the upstream dump against production PostgreSQL or an existing ACE database: its database recreation statements are destructive. Do not regex-parse SQL. Complete-world conversion requires the explicit API below; weenie-only extraction does not imply whole-world support.

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

## Complete world conversion

`import_complete_world(&mut MariaDbStaging, dump)` now extracts the unchanged
complete official Patches release into `CompleteStagedWorld`. The existing
weenie-only `import_staged_world` remains strict and rejects populated world
systems. The complete API accounts for all 54 schema tables: 24 weenie/property
tables and 30 explicitly typed world-row families (instances/links, encounters,
events, house portals, points of interest, quests, spells, recipes and their
requirements/modifiers, treasure tables, version). Empty tables remain counted.
Unknown populated tables, unknown columns, count discrepancies and lossy SQL
warnings fail. The exact MariaDB 12 `@@sql_notes` deprecation diagnostic is
allowed because it changes diagnostic configuration only; data warnings are
still rejected. SQL input remains unchanged and executes only in a disposable
private server.

The frozen world rows retain source primary keys, timestamps, signed values,
nullable values, bit booleans and authored ordering. Mappings come from the
checked-in pinned `data/world-base.sql`; `data/generate_world_schema.py` generates
Rust declarations from that schema definition only. It never reads input data SQL.
Legacy extraction uses JSON solely as bounded MariaDB/tool interchange. Native
packs contain typed binary records. Full data conversion is not implemented
magic, crafting, housing or quest runtime behavior.

The verified v0.9.295 archive has SHA256
`fd35cff8b2cea8408ae13839b9b1862c30452a1013f43eb62ec83c25349bd148`; its unchanged
SQL SHA256 is `98caa038a27d2620bc4b644e62150aa4eec7754251c6313aaa959c3b11f784a8`.
Actual isolated extraction and aggregate pack compilation passed on Linux:
43,913 weenies and 915,050 non-weenie rows. Run explicitly:

```sh
BACE_MARIADB_BASEDIR="$PWD/.local/mariadb/usr" BACE_WORLD_SQL=/path/to/ACE-World-Database-v0.9.295.sql cargo +stable test -p bace-import --test complete_world -- --ignored
BACE_MARIADB_BASEDIR="$PWD/.local/mariadb/usr" cargo +stable run -p bace-import --example build_world -- /path/to/world.sql /path/to/unpublished-pack-directory
```

`inspect_world_references` reports unresolved authored references with exact
counts and bounded deterministic examples. It preserves them rather than dropping
rows or rewriting upstream content. Runtime activation must resolve the relevant
required references and geometry before admitting an instance. Generator values
also require their ACE treasure/placement flags to determine their meaning.

## EmoteScript and loot files

`import_emote_script` / `export_emote_script` are bounded native Rust authoring
boundaries, with named/positional fields, nested branches, ranges, movement/heading
shorthand and exact i64 values. Compilation returns no partial result on errors.
Export emits flat explicitly linked sets; relational IDs remain in native TOML.
Quest/event nested branches receive distinct `@` keys as understood by pinned ACE.
Full upstream grammar/decompiler parity and external display-name lookup are not
claimed. Tests cover nested links, widths, malformed input and movement ordering.

Command metadata comes from ACEmulator/EmoteScript at
`aa22635cecc32f534d3630882e96dada38dd6f6d`; numeric content IDs remain pinned to ACE.
See `data/emotescript-schema.provenance.toml`, `generate_emote_schema.py`, and
`docs/licenses/EmoteScript-LGPL-3.0.txt` for retained attribution/license. No C#
process/library is required to run these Rust tools.

`import_loot_json`, `export_loot_json` and `export_loot_sql` target the pinned
`TreasureDeath` profile fields. Native `DeathTreasureV1` is separate TOML content;
legacy JSON inputs are capped at 64 KiB and reject missing/duplicate/unknown fields.
SQL export emits one INSERT; destination assigns row ID/timestamp. No SQL is
executed by export. Loot SQL import and binary/runtime publication are unsupported.

## CustomClothingBase interchange

`import_clothing_json` / `export_clothing_json` preserve the custom table's setup
variants, ordered part/model changes, every texture mapping, palette templates,
icons, ordered palette effects and color ranges. `PaletteSet` accepts both actual
palette-set DIDs (`0F`) and the mod's direct-palette extension (`04`). Native
storage uses the separate `ClothingPatchV1` TOML schema.

Input is capped at 1 MiB and 4,096 entries per JSON collection. Names are case
insensitive; decimal/0x strings and numeric uint32 values are accepted. Trailing
commas outside strings are supported. Duplicate keys/normalized numeric identities,
unknown fields, invalid DID types, overflow and out-of-limit effects reject the
entire document. Template order survives JSON import/export for first-template
fallback behavior. Exports include all changed setup/template entries, not a flattened
single part. A matching setup/template replaces that entire entry when merged;
unmentioned base entries remain. Fresh resolution always starts from original DAT.

Format/extension reference is OptimShi/CustomClothingBase at
`122145d0f6d0c159183f0f229aae611085a67bf1`; see
`data/custom-clothing-base.provenance.toml`. No upstream source or examples are
redistributed. Synthetic tests independently assert multi-part/range values,
key-level merge expectations, order and malformed input rejection. Supplied-DAT
Studio tests additionally consume the three upstream examples from a local checkout.
This is legacy tooling support, not a new official ACE protocol requirement.
