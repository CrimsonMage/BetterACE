# bace-storage-codec

Also implemented: BACE version-1 immutable indexed `.bace` packs, streaming compilation, read-only mmap, bounded lazy integrity checks/scans, retained borrowed handles, base/delta/tombstone overlays and immutable manifests. See [pack-format.md](../../../../docs/pack-format.md) for layout, operating requirements and the sole audited mapping exception.

Pack payloads are currently independently bounded opaque bytes. Complete typed world views, DAT geometry packing and gameplay cache integration remain separate work. Cold mapped reads MUST run off the simulation thread. Immutable identical publication retries verify complete bytes with bounded buffers; the application owns durable manifest acceptance and generation activation.

Implemented: generic frozen-DTO postcard encoding/decoding with bounded output and a checked envelope. Default payload cap is 16 MiB; callers must also validate domain limits before accepting decoded state.

V1 envelope is exactly: `ACERBIN\0` (8 bytes), envelope version/kind/schema/flags (four little-endian u16), payload length (little-endian u32), SHA256 (32 bytes), postcard payload. Header length is 52 bytes. Digest covers the first 20 header bytes plus payload. It detects accidental corruption; it does not authenticate a writer.

`inspect` validates magic, envelope version, flags, length, limit and digest. `decode` additionally validates aggregate kind/schema and rejects trailing postcard bytes. Unsupported versions fail explicitly. Domain DTOs must remain frozen; gameplay structs are not storage schemas. Future migrations require explicit old/new DTOs and tests.

Active generations are now capped at `MAX_ACTIVE_PACKS = 3` (one base plus two
deltas), even if caller limits are larger. `PackGeneration::compact` streams the
active overlay into a new immutable base with at most one retained record handle
per segment. Newest rows win, tombstones disappear, cancellation leaves the old
generation usable and old borrowed handles remain valid. The caller runs it on
the pack-I/O owner and handles durable manifest acceptance/reclamation; temporary
and retained historical files are distinct from the active-generation limit.


Frozen gameplay kinds 100–103/schema1 cover players (including character metadata
and quests), items, corpses and housing/access/rent state. They embed only frozen
WeenieV1 values, not evolving runtime structs. Dedicated encoders validate identity,
finite properties, collection bounds and a 2 MiB payload cap; unsupported schema
versions require an explicit migration. Relational owner/name checks remain in the
PostgreSQL adapter, while simulation owns live state and mutation revisions.

Schema2 adds explicit item/corpse placement and nullable house ownership with an
access generation and rent-payment state. V1 item/corpse migration requires an
explicit trusted placement; missing containment is never interpreted as deletion.
V1 housing migration marks rent data incomplete until an authoritative supplement
is supplied. Existing V1 field order and layouts remain unchanged.

The current schema3 wrappers retain the full prior schema and add character UI or
item/corpse/house enchantment supplements. Runtime lifecycle/freezer paths retain
these wrappers rather than flattening them to V1/V2. Database identity gates reject
schema downgrades and rare-state identity/counter rewinds. Enchantment expiry and
removal remain legitimate typed mutations, not an immutable-field restriction.
The corpse V5 access transition permits a legacy unknown V3/V4 source identity to
be filled once at a newer revision, then freezes that source and its derived
victim/monster rights. A V5 codec regression and real PostgreSQL V3-to-V4
transfer test cover the migration; later identity changes remain rejected.

Unreleased player schema 4 adds cast recovery with an explicit capture clock and
bounded contract records; migrations retain all prior bytes without inventing a
cast history. A same-revision recovery snapshot cannot change spell school,
reset countdowns, rewind history, or age faster than elapsed trusted UTC. Slower
aging is admitted so a fixed-step scheduler stall cannot block an otherwise valid
save. This is a persistence monotonicity bound, not GDLE runtime timing parity.
`tests/player_v4.rs` covers migration, malformed input and both stall/early-clear
boundaries.

The unreleased NPC workflow V1 has frozen per-effect DTOs, distinct local and
player aggregate revision fences, and explicit adopted/detached markers. Queued
experience is appended as a separate effect with AwaitingAdmission/Ready phases;
it cannot be marked adopted or detached in the wrong phase. Ready queue admission
is workflow-only; a subsequent recipient effect carries the actual player delta.
Runtime must persist that player delta with its adopted continuation marker in
one transaction and restore participant owners before permitting VM execution.

Unreleased NPC workflow V3 preserves frozen V1/V2 migrations and adds a monotone
canonical source-head version, original template locator, exact manifest/program
hashes, live/archived source state, source quest progress and accepted location.
Its inventory proof fences the source aggregate and complete held gear forest,
including exact enchantments, death-drop membership and generated-parent
provenance. The bounded payload is 4 MiB; mutable gameplay structs are converted
explicitly rather than serialized. Registry CAS counters belong to the old world
incarnation: a new owner restores exact entries but rebases these counters to zero,
retaining durable item mutation and SQL versions. Legacy unknown locators remain
unknown and require an authoritative source lookup before admission.

Unreleased corpse schema 4 retains V3 verbatim and adds the source actor and
simulation death operation as an all-present or all-absent pair. Fresh saves
require both for live restoration. V1–V3 migrations preserve unknown identity;
the opaque durable journal key is not parsed to invent a simulation operation.
Runtime keeps those legacy corpses unadmitted until authoritative identity is
available. Expiry and region unload retain the V4 wrapper and exact deadline.
`tests/corpse_v4.rs` covers legacy migration, identity pairs and corrupt envelopes.


Unreleased item schema 4 retains V3 verbatim and adds an optional frozen
Creature/Cow construction companion. It preserves factory subtype, immutable
keyed generator provenance, source equipment order and topological death-roster
identity. Each member list is bounded to 1,024 entries while decoding; total item
payload remains at most 2 MiB. Children retain their own item aggregates and
placements. No mutable inventory, prepared physics profiles or transient action
queues are serialized. Legacy migration leaves construction explicitly unknown;
runtime must hold a creature-valued save without the required companion.
`tests/item_v4.rs` verifies byte-preserving V1–V3 migration, malformed subtype and
ordering rejection, envelope integrity and immutable-origin transitions. These
are storage invariants, not a claim of complete ACE creature reconstruction.

Unreleased item schema 5 wraps V4 with the nullable pinned ACE CreateList
`DestinationType` flags. Legacy migration keeps the origin unknown; it never
infers Treasure eligibility from current placement or GeneratorId. Once known,
the origin cannot be changed by an ordinary item save. This frozen schema and
`tests/item_v5.rs` establish the storage contract only; acquisition, every item
writer and player NoCorpse selection must carry it before schema 5 is enabled
for live items.

Corpse V4 deadline identity now permits a strictly revision-advancing decrease
for final-item pickup, while rejecting extension or a same-revision rewrite.
Source/op/owner identity remains immutable. This is the persistence rule for
ACE WorldObject_Decay's empty-corpse clamp; runtime joins it to the exact pickup
transaction and tests preserve all unrelated corpse fields.

Vendor stock kind 105/schema 1 is an unplaced, bounded owner marker. It freezes
the vendor and marker IDs, accepted source revision/hash, loaded state, ordered
default and unique stock tree IDs, display counts, source contribution units,
unique sale time and stock revision. Every referenced item remains an ItemSaveV5
aggregate; the marker never stands in for an item or world creature. The Postgres
adapter validates the complete relational tree in the same transaction as stock
publication. Commerce output and cold restoration remain separate runtime work.
