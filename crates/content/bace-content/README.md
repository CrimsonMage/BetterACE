# bace-content

Implemented: frozen `WeenieV1` DTO, all upstream Weenie property collection families, structural validation and immutable catalogs. Does not implement gameplay or validate every cross-weenie/DAT reference yet.

`Property<T>` uses numeric IDs with typed values and preserves unknown IDs. Dictionary families sort by ID for canonical output; authored sequences (including emote actions, generators, create lists, visual overrides and pages) preserve order. Optional fields distinguish absent from zero. Signed spell/body/skill identifiers and typed legacy authoring metadata are retained. Collection limits are enforced during decoding, with total-entry and string limits checked before acceptance. Numeric enum values stay numeric.

`Catalog::prepare` validates a complete batch and builds every index without mutating active state. It can run on a cloned catalog off the world thread. `publish` rejects stale preparations and swaps a single Arc at the world boundary. Indexes cover ID, exact class name, type and display name; misses are not negatively cached. Existing instances retain their template Arcs. Durable journal acceptance is the application's responsibility.

DTO field inventory derives from official ACE.Entity.Models at `47edade3bd3f6044b676d4eb877c4965c7eda62b` (AGPL-3.0-only). V1 fields and serialization order are frozen. New layouts require an explicit schema version and migration. SQL page/action sequence IDs are retained as optional legacy metadata. No stock-client or complete world-import compatibility claim follows from these tests.

`CharacterStartProfileV1` preserves native starter-gear/spell order and named
starting-area spell references. `CreatureNameIndexV1` retains template IDs and
display names for the pinned `WorldDatabaseWithEntityCache` creature-name query.
Both are frozen schemas with bounded collections and explicit version checks.
The name index permits cold startup policy preparation without decoding the world.

`WorldRecordV1` now preserves the 30 pinned non-weenie world-table families in
explicit frozen typed row variants. Enum ordering and field ordering form binary
schema 1 and must not be changed without a migration. Source primary keys,
nullable fields, signed values, timestamps and flags are retained. Finite numeric
and bounded string validation runs before pack encoding and after decoding.
`LandblockIndexV1` is a separately bounded frozen lookup DTO containing instance
and encounter IDs. `inspect_world_references` supplies deterministic missing-ID
counts/examples without modifying source data; a presence diagnostic is not a
claim that every generator field is semantically a plain weenie reference.

`DeathTreasureV1` is a separate frozen TOML authoring schema for the 16 content
fields of pinned `ACE.Database/Models/World/TreasureDeath.cs`. It validates schema
version, identity, tier, finite quality and chance/quantity ranges. It intentionally
has no destination database row ID/timestamp. It is not part of `WeenieV1` or the
weenie-only binary pack; any runtime binary integration needs its own versioned
codec/publication contract. Studio owns the editing workflow; `bace-import` owns
legacy loot JSON/SQL files. Unknown fields reject input.

`TreasureTableSetV1` is a frozen typed source-table schema with ordered heterogeneous
rows, exact `f32` bit patterns, reference validation, spell routes and bounded
mutation scripts. It is distinct from native `LootGraphV1` because ACE's
ChanceTable quality behavior is part of the pinned algorithm.

`ClothingPatchV1` owns native ClothingBase overrides: multiple setup variants,
ordered part/model/texture changes and palette templates with direct palettes or
palette sets and multiple ranges. `resolve_clothing` replaces whole matching
setup/template entries, preserves unrelated originals and resolves each revision
from the original DAT table; it never stacks stale prior overlays. A new ID may
resolve without a DAT ClothingBase when all referenced assets exist. Range counts
are in colors; 2,048 is wire-representable through the zero-length byte convention.
`validate_assets` validates references from the supplied immutable index, with no I/O.
Studio/importer impose a 1 MiB document limit; namespace 50 accepts the native
patch, while game-world application remains a separate integration gate. OptimShi's extension is an explicit legacy tooling
format, not the authority for official ACE wire behavior.

`AnimationSwapPatchV1` is a separate schema-1 native overlay for DAT animation
`ReplaceObject` hooks. Ordered insert/replace/remove edits target frame and hook
indices; the original DAT animation is never rewritten. This is authoring data,
not an assertion that a live server applies the overlay.
