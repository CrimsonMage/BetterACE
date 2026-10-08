# bace-content-tools

Implemented APIs: `parse`, `compile`, `compile_template`, `decode`, `export`, `export_binary`. Errors are explicit, and unknown fields are rejected.

Native authoring is TOML; see `tests/fixtures/content/minimal.toml` at the workspace root. Compilation validates, canonicalizes dictionary order, and wraps a frozen typed postcard DTO in the `bace-storage-codec` envelope. Export produces equivalent native TOML. Sources and payloads are capped at 16 MiB, with additional collection/string validation.

Kind 1/schema 1 identifies WeenieV1. Binary content is an application format, not an ACE packet format. Schema migrations are not invented: unsupported schemas fail until a reviewed migration exists. Publication and SQL are owned by application/storage crates.

Character-start TOML compiles to kind 22/schema 1 with a 1 MiB payload limit.
`compile_character_start` is also available as an example command taking a TOML
path and a new immutable `.bace` output path. Complete world compilation adds
namespace 48/key 1, kind 23/schema 1: a bounded creature-name index keyed by
template identity, and namespace 49/key 1, kind 24/schema 1: the unique template
class-name index. Both encoded payloads are bounded to 8 MiB. Offline index validation checks complete source membership;
legacy packs without it require recompilation before enabling character creation.

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
composition only. Runtime publication, cache composition,
Windows/macOS validation and whole-database performance measurements remain work.

`build_world_pack` accepts complete typed weenies and `WorldRecordV1` rows and
writes one aggregate base `.bace` file plus an immutable manifest. It compiles one
bounded payload at a time, retaining borrowed sort keys and landblock ID indexes.
It rejects duplicate record identities, inconsistent generated landblock IDs,
oversized inputs and cancellation. `compile_world_record`/`decode_world_record`
use kind 2/schema 1; `decode_landblock_index` reads kind 3/schema 1. Weenie namespace
1 is unchanged, namespace 2 indexes landblocks, and namespaces 16–45 follow the
frozen world table inventory. Pack layout, exact namespace ordering and real-world
count evidence are documented in `docs/pack-format.md`. This is an offline
compiler; durable journal acceptance and runtime asset admission remain owned by
the application. The full official release test builds a single aggregate file,
not a directory of per-weenie/per-instance files.


Namespace3/kind4 stores bounded parent-instance link indexes. Before a pack is
accepted, `validate_world_pack_indexes` proves complete, ordered membership against
source records. It rejects missing, empty, duplicate, misdirected and extra indexes
while retaining only one index and one source row. Source-authored dangling child
GUIDs remain intact for later DAT/runtime resolution. The scan belongs to tooling
acceptance; runtime startup still reopens the accepted indexes lazily.

Native nested loot and rare policies use TOML `LootGraphV1` / `RareProfileV1`,
frozen binary kinds 20/21 schema 1, runtime namespaces 46/47. `build_loot_pack`
compiles multiple tables/profiles into one aggregate `.bace` supplement; it does
not activate it. Decode validates bounds, graph cycles and explicit probability
configuration. Runtime publication must additionally check referenced templates.
The disabled rare example deliberately supplies no inferred retail probability.
Pinned ACE chance tables, references, enum/spell indexes and mutation scripts
are a separate `TreasureTableSetV1` TOML source compiled to kind 27/schema 1
in namespace 52. The world compiler embeds the pinned profile as one indexed
record in the aggregate `.bace` base; the accepted generation owns runtime data.

`parse_clothing_patch` and `compile_clothing_patch` validate native ClothingBase
TOML and write a kind-25/schema-1 envelope for namespace 50. Runtime inbox
review uses these codecs alongside typed world rows; game-world application of
ClothingBase overrides remains a separate integration gate.

`parse_animation_swap`, `compile_animation_swap` and `decode_animation_swap`
handle the bounded native ReplaceObject overlay (kind 26/schema 1, offline
namespace 51). The patch applies to a decoded DAT animation through
`bace-asset-overlay`; runtime publication remains unsupported until every
motion preparation path reads the accepted overlay generation.
