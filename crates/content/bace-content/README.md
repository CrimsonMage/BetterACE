# bace-content

Implemented: frozen `WeenieV1` DTO, all upstream Weenie property collection families, structural validation and immutable catalogs. Does not implement gameplay or validate every cross-weenie/DAT reference yet.

`Property<T>` uses numeric IDs with typed values and preserves unknown IDs. Dictionary families sort by ID for canonical output; authored sequences (including emote actions, generators, create lists, visual overrides and pages) preserve order. Optional fields distinguish absent from zero. Signed spell/body/skill identifiers and typed legacy authoring metadata are retained. Collection limits are enforced during decoding, with total-entry and string limits checked before acceptance. Numeric enum values stay numeric.

`Catalog::prepare` validates a complete batch and builds every index without mutating active state. It can run on a cloned catalog off the world thread. `publish` rejects stale preparations and swaps a single Arc at the world boundary. Indexes cover ID, exact class name, type and display name; misses are not negatively cached. Existing instances retain their template Arcs. Durable journal acceptance is the application's responsibility.

DTO field inventory derives from official ACE.Entity.Models at `47edade3bd3f6044b676d4eb877c4965c7eda62b` (AGPL-3.0-only). V1 fields and serialization order are frozen. New layouts require an explicit schema version and migration. SQL page/action sequence IDs are retained as optional legacy metadata. No stock-client or complete world-import compatibility claim follows from these tests.
