# ACE mutation scripts and independent oracle

Authority: official ACEmulator/ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b`.
The original source files and script resources are licensed AGPL-3.0-only,
copyright ACEmulator/ACE contributors. `source.sha256` identifies unchanged C#
classes and enum files; `scripts.sha256` identifies all 51 imported active melee,
missile, caster and armor scripts. No proprietary assets or player data are included.

`import_scripts.py` imports exact upstream compatibility inputs and generates
`src/treasure_mutations/pinned.rs`. These inputs are an upstream format adapter,
not a new native authoring format. `MutationScripts::pinned` parses them into
bounded immutable structures during preparation, before simulation ownership.
Native content authoring remains TOML.

The oracle compiles upstream `MutationCache` and all eight mutation/effect classes
unchanged; only logging, WorldObject property access and explicit random draws
are harness stubs. It independently parses embedded resources, preserving decimal
chance accumulation, source enum resolution, correlated outcome rolls and typed
arithmetic. The Rust evaluator is not linked into or used to generate fixtures.

With Python 3 and .NET SDK 8.0.408:

```
python3 crates/gameplay/bace-loot/oracle/mutations/import_scripts.py
python3 crates/gameplay/bace-loot/oracle/mutations/extract_oracle.py
dotnet run --project crates/gameplay/bace-loot/oracle/mutations/Oracle.csproj --configuration Release > crates/gameplay/bace-loot/tests/fixtures/ace_mutations.tsv
cargo +stable test -p bace-loot --test treasure_mutations
```

1,836 golden rows cover every active script, all eight tiers plus tier fallback,
four controlled variates, exact draw counts, integer properties and floating
properties. Additional regressions cover rollback, unavailable property inputs,
source AssignDivide multiplication, zero division, malformed inputs and bounds.

Scope is the active loot scripts. Recipe-specific property enums, variable
arguments (unimplemented upstream), and arbitrary mutation resources are rejected
at preparation. Non-finite arithmetic, unsupported quality names and overflowing
float-to-int conversion are explicit errors; no partial item/cursor is published.
