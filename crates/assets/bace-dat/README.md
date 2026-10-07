# bace-dat

Status: foundation with bounded archive, terrain and character-table readers.

`DatArchive` reads DAT headers, B-tree records and checked sector chains. `Landblock` decodes outdoor terrain. Setup, motion, BSP and dungeon geometry decoding remain incomplete; missing collision/motion assets still block world entry.

Character/progression resources now decode the complete pinned ACE layouts:

- `XpTable` (`0x0E000018`): inclusive attribute/vital/trained/specialized rank tables, 64-bit character level XP and level skill credits.
- `SkillTable` (`0x0E000004`): packed hash-table entries, UTF-8 PStrings with record-relative DWORD alignment, signed trained/specialized costs, six formula words and finite double bounds/modifiers. Specialization cost includes training cost. The parser does not call ACE's separate `AddRetiredSkills` helper or synthesize absent entries.
- `CharGen` (`0x0E000002`): starter areas/positions, heritage credit budgets, creation skill overrides, templates, gender metadata and full nested appearance/gear choices. SmartArray counts, BinaryReader strings, compact known-type IDs, palette expansion and coordinate/quaternion ordering follow the source. Heritage costs are creation overrides and must not replace SkillTable costs for later training. Appearance/outer marker bytes skipped by ACE are preserved without invented version semantics.

Each type exposes `decode(bytes)` and `decode_with_limits(bytes, limits)`, plus `load_verified(archive, DatTableVersion)`. Loading requires a Portal dataset, exact caller-approved engine/game versions, and exact record iteration before reading. The records do **not** contain an independent schema-version field. `DatTableVersion` matching is not fingerprint admission: production composition must independently verify the immutable DAT against an approved asset manifest before trusting those expected values. The ordinary `decode` APIs validate layout/record ID, not source authenticity.

`DatTableLimits` defaults to 16 MiB per record, 65,536 entries and 1 MiB per string. CharGen additionally shares one aggregate nested-entry budget. Counts are bounded against remaining bytes before allocation; malformed offsets/counts, duplicate keys, negative XP counts, nonfinite numeric values, unexpected table IDs and trailing bytes return errors. Decoders do not perform world mutation, infer gameplay formulas, spend XP or accept a client-selected cost.

Provenance: official ACEmulator/ACE at `47edade3bd3f6044b676d4eb877c4965c7eda62b`, `ACE.DatLoader/FileTypes/{XpTable,SkillTable,CharGen}.cs`, `Entity/{SkillBase,SkillFormula,HeritageGroupCG,TemplateCG,SexCG,ObjDesc,...}.cs`, `BinaryReaderExtensions.cs` and `UnpackableExtensions.cs`. All upstream attribution and AGPL-3.0-only licensing are retained. [Official DAT loader](https://github.com/ACEmulator/ACE/tree/47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.DatLoader) is authoritative.

Independent golden evidence is in `tools/bace-compat/fixtures/dat.json` and `tests/dat_vectors.rs`. `oracle/dat_generate.py` verifies every C# input against immutable pinned GitHub bytes, compiles **unmodified** official table/nested decoders, and records source hashes. The fixture builder uses only synthetic records, including multi-byte string/count boundaries, Unicode strings, negative skill cost preservation, extended resource IDs and 64-bit XP. The sole dependency stub is an unused Frame convenience-constructor parameter type; it does not parse bytes. These tests compare every exposed synthetic table/appearance field and exhaustively reject truncated synthetic inputs.

`cargo test -p bace-dat` covers malformed/limited records and explicit archive/version/iteration rejection. For supplied assets, set `BACE_DAT_DIRECTORY` and run `cargo test -p bace-dat --test tables supplied_portal -- --ignored --nocapture`. That explicit smoke suite fingerprints the user's archive, loads all three tables and checks sampled truncations across actual records. It has passed on the supplied Linux archive (191 attribute entries, 197 vital entries, 209 trained entries, 227 specialized entries, 276 levels, 38 skills, 13 heritages). No proprietary bytes are committed. This is table-format evidence, not full stock-client character creation or movement acceptance.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.
