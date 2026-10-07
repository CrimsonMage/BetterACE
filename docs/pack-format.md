# BACE mapped pack format, version 1

Implemented foundation: streaming compilation, independently bounded opaque records, immutable `.bace` files, read-only mmap, cursor scans, base/delta lookup and versioned manifests. Complete typed PY16 world views, DAT collision compilation and gameplay cache integration remain separate work. The existing `ACERBIN` individual-aggregate/save envelope is unchanged.

## Ownership and safety

`bace-storage-codec` owns generic mapping/framing/indexes. `bace-content-tools` supplies sorted world records; `bace-dat` will supply DAT-derived collision records. `bace-tooling` owns compilation/publication workflows and `bace-runtime` schedules I/O workers. `bace-content` owns template/index semantics; `bace-world` owns active regional bundles; `bace-physics` consumes prepared collision inputs on the one simulation thread.

The application MUST own the pack directory and preserve published files unchanged. No process may modify/truncate a backing file while mapped. Read-only descriptors, copy-on-write and checksums do not enforce that assumption on all operating systems. Arbitrary externally mutable files are unsupported. Compilation publishes new content-addressed files after flushing and closing writable handles; it never overwrites an existing target. Idempotent retries compare complete candidate/existing bytes with bounded buffers; mismatches error rather than repair in place. Files are flushed, and Unix parent directories are also flushed. Windows directory-entry crash durability remains unqualified.

Workspace `unsafe_code = "forbid"` remains unchanged. Only this crate declares its own `unsafe_code = "deny"`, preserving clippy denies. Private `mapping.rs::open_immutable` contains one reviewed constructor call with a local allow and safety contract. All parsing and borrowed views use safe checked offsets. Architecture checks must enforce that exact exception. No Linux-only mapping facility is required.

`RecordHandle` retains `Arc<MappedPack>` plus checked offsets. Its byte borrow cannot outlive the handle. Old handles survive generation changes. Arc pins lifetime, not physical RAM: cold mmap access can fault/block. Compile/open/lookup/validation belong on workers; physics consumes bounded prepared active bundles. There is no real-time paging guarantee.

## Layout

All generic integers are unsigned little endian. Offsets are absolute u64 byte offsets. Initial supported deployments have 64-bit address spaces. Files contain `header | payloads | index pages | root directory`. No native pointers, Rust layouts, `usize` or OS paths appear on disk. Domain schemas must separately preserve f32/f64 bits and signed property widths.

Header is 128 bytes:

| Offset | Bytes | Meaning |
|---:|---:|---|
| 0 | 8 | ASCII `BACEPACK` |
| 8 | 2 | version 1 |
| 10 | 2 | header length 128 |
| 12 | 4 | reserved zero |
| 16 | 8 | total file length |
| 24 | 8 | record count including tombstones |
| 32 | 8 | root-directory offset |
| 40 | 8 | root-directory byte length |
| 48 | 8 | index-page count |
| 56 | 40 | reserved zero |
| 96 | 32 | generation digest |

Generation digest is SHA256(header bytes 0..96 followed by root-directory bytes). Filename is lowercase digest hex plus `.bace`. This is a hierarchical content digest, not whole-file SHA256: roots commit to index pages, whose entries commit to payloads. It detects corruption, not authorization of a writer.

Key (16 bytes): `namespace:u16 | reserved:[u8;6]=0 | logical_id:u64`. Sort numerically by `(namespace,id)`, not the little-endian bytes. Namespace assignment belongs to domain schemas.

`bace-content-tools::build_weenie_pack` assigns namespace **1** to weenie records,
with schema **1** and the weenie ID as logical ID. Payloads are the existing
kind-1/schema-1 `ACERBIN` WeenieV1 envelopes; no evolving gameplay layouts are
introduced. The builder decodes one TOML source at a time, spools binary payloads
to disk and sorts compact ID/offset/length entries. Its reopened mmap lookup test
is in `crates/content/bace-content-tools/tests/packs.rs`. This authoring pipeline
does not replace the runtime's existing in-memory catalog or accept a PostgreSQL
generation head. Pack activation and bounded gameplay caches remain required.

Root entry (80 bytes): `first_key[16] | last_key[16] | page_offset:u64 | record_count:u32 | reserved:u32=0 | page_sha256[32]`. Pages hold 1..32 entries, with ordered disjoint key ranges and contiguous page bytes immediately before the root directory. Startup verifies only the bounded root directory, counts, bounds and header digest. It does not scan index pages or payloads or request mmap prefaulting.

Record entry (72 bytes): `key[16] | flags:u16 | schema:u16 | reserved:u32=0 | payload_offset:u64 | payload_length:u64 | payload_sha256[32]`. Flags 0=present, 1=tombstone; other flags reject. Tombstones have zero bytes and the empty-sequence digest. Empty present records remain distinct. Payload ranges must be between header and index; overflow, bad flags, unordered/duplicate keys and page/root disagreement reject.

Lookup binary-searches roots, verifies one index page, and verifies the selected payload. Generic validation does not replace domain checks on typed fields, strings, references and finite numeric values.

## Limits and streaming

Default limits: file 64 GiB; payload 16 MiB; root directory 8 MiB; 3,000,000 records; 64 total segments; scan 1,024 records and 64 MiB payload hashing per call. The smallest effective limit wins.

`compile_pack` streams one owned record at a time, writing payloads to an output temp file and index pages to another. Memory is one record, one page, bounded root directory and fixed copy buffers. It does not sort: callers must supply ordered streaming/external-sort inputs rather than load the world first. Index copying uses 64 KiB; retry comparison uses two 64 KiB buffers.

`scan(after,limit)` returns physical records including tombstones; the last returned key is the next exclusive cursor. The payload-byte budget may shorten a batch. An oversized first record errors rather than skipping. Merged generation enumeration and compaction are not implemented by this API.

## Manifests and publication

Manifest binary: `BACEGEN\0[8] | version:u16=1 | reserved:u16=0 | segment_count:u32 | generation_revision:u64`, then count entries of `pack_generation_digest[32] | record_count:u64`, then SHA256 of all preceding bytes. Base is first; deltas follow oldest to newest. Counts and complete length are checked before allocation. Filenames are reconstructed canonically from digests.

Manifest filename and `content_hash()` use SHA256 of the **complete encoded manifest**, including its trailing digest, plus `.manifest`. Manifest revision/hash and an individual pack digest are distinct identifiers. `write_manifest` is immutable and idempotent; it never changes a current pointer. The application must journal accepted manifests durably and enforce revision monotonicity. Restart opens accepted files without rebuilding. Missing accepted files error; unaccepted orphan files do not become world content.

`PackGeneration::lookup` checks newest segments first, stopping on a record or tombstone and falling through only on missing. Empty base packs work. Misses are not cached. Old files cannot be collected while any accepted/recovery manifest or handle needs them; Windows cleanup must tolerate open mappings.

Secondary indexes MUST agree with the primary generation. Appending a changed name/type leaves stale previous memberships unless the domain compiler emits removals or validates candidate index results against current primary revisions. Primary newest-wins lookup alone is insufficient for atomic secondary-index replacement.

## Evidence and remaining gates

`tests/packs.rs` includes an independently computed Python struct/hash golden, deterministic compile/retry, bounded scans, empty records/tombstones, old-handle retention, manifest restart, malformed ordering/limits, lazy index/payload corruption and authenticated malformed offset rejection. These synthetic tests do not establish ACE compatibility.

Run the same goldens on Windows, Linux and macOS; report only platforms actually tested. Complete typed world schemas, collision companions, runtime cache/publication integration, compaction, cross-platform crash qualification and measured memory/startup/load budgets remain required. The current generic container does not establish that existing gameplay stopped retaining its decoded catalog.
