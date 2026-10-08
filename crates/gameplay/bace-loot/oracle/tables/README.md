# Original ACE table evidence

The authority is official ACEmulator/ACE at
`47edade3bd3f6044b676d4eb877c4965c7eda62b`. Original ACE contributors retain
copyright; derived tables and fixtures use AGPL-3.0-only. `extraction.json`
records every input source hash. No proprietary DAT data is included.

From the repository root:

```sh
python3 crates/gameplay/bace-loot/oracle/tables/generate.py
python3 crates/gameplay/bace-loot/oracle/tables/verify.py --dotnet /path/to/dotnet
python3 crates/gameplay/bace-loot/oracle/tables/template_closure.py
python3 crates/gameplay/bace-loot/oracle/tables/export_toml.py
cargo +stable fmt --all
cargo +stable test -p bace-loot --test ace_tables
```

`generate.py` produces independent fixture inputs and temporary Rust/index
extraction artifacts. `export_toml.py` combines the verified rows, indexes and
official mutation scripts into the native table-set authoring source. Ordered
chance rows and enum declaration order are preserved. The world
compiler stores that source as one versioned record in the accepted `.bace`
generation; production loot code does not compile the generated Rust tables.
`verify.py` independently compiles the original C# static field initializers and
enum declarations, then reflects 10,703 rows. Python-evaluated numeric constants
never enter that C# oracle. The Rust test compares order, values, exact f32 bits,
reference targets and complete row counts.

The enum and spell-progression indexes use checked fixed-record offsets and
UTF-8 pools, explicit format signatures, bounded lengths and CRC32 payloads.
Their integrity tests check the compiled immutable assets. They are source tables,
not evolving gameplay saves or a claim of .NET random-sequence equivalence.
