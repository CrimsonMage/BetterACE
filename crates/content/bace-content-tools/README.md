# bace-content-tools

Implemented APIs: `parse`, `compile`, `compile_template`, `decode`, `export`, `export_binary`. Errors are explicit, and unknown fields are rejected.

Native authoring is TOML; see `tests/fixtures/content/minimal.toml` at the workspace root. Compilation validates, canonicalizes dictionary order, and wraps a frozen typed postcard DTO in the `bace-storage-codec` envelope. Export produces equivalent native TOML. Sources and payloads are capped at 16 MiB, with additional collection/string validation.

Kind 1/schema 1 identifies WeenieV1. Binary content is an application format, not an ACE packet format. Schema migrations are not invented: unsupported schemas fail until a reviewed migration exists. Publication and SQL are owned by application/storage crates.

`build_weenie_pack` compiles a bounded list of native TOML sources into an
immutable indexed `.bace` file and binary generation manifest. Namespace 1 /
schema 1 contains the existing kind-1/schema-1 WeenieV1 envelopes, keyed by
weenie ID. Sources are decoded one at a time into a temporary binary disk spool;
only ID/offset/length entries are sorted in memory. Duplicate IDs, invalid
content, cancellation, more than 100,000 sources or payloads above 8 GiB fail.
The caller owns an unpublished output directory and cleanup on failure.

Tests compile unsorted inputs, reopen the pack with the existing read-only mmap
reader, lookup individual records and decode their expected identities; duplicate
and cancellation tests ensure no pack is published. This establishes tooling
composition only. Runtime publication, cache composition, full-world conversion,
Windows/macOS validation and whole-database performance measurements remain work.
