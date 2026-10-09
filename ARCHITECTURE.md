# BetterACE — BetterACEmulator architecture

**BetterACE** is the project and repository name for **BetterACEmulator**, a Rust reimplementation of official ACEmulator/ACE, targeting the unmodified Asheron's Call client. This document is normative. `AGENTS.md` specifies contributor requirements, `architecture.toml` assigns crate ownership and permitted dependencies, and `docs/implementation-status.md` records actual capability. A scaffold is not an implementation or a compatibility claim.

BetterACE uses **AGPL-3.0-only** across the Rust workspace. The full license is in `LICENSE`; contributor requirements are in `AGENTS.md`.

## Reference and scope

**BASE-01:** The protocol and gameplay reference MUST be official `https://github.com/ACEmulator/ACE` at `47edade3bd3f6044b676d4eb877c4965c7eda62b`. Local forks are not the baseline. Baseline updates MUST be explicit changes with regenerated fixtures and reviewed behavior differences. Pins are in `docs/baselines.toml`. By explicit project-owner direction (2026-10-07), pinned GDLE is the primary behavioral reference for server-side door and monster authority: state, collision-sensitive transitions, AI/movement ownership and attack eligibility. Official ACE remains the wire-layout baseline and default reference for other gameplay. Player casting movement/release also follows pinned GDLE. By subsequent owner direction, physical and spell combat follow verified retail evidence first, pinned GDLE next, with explicit reviewed ACE gap-fills; defects are documented rather than copied as requirements. Allegiance XP follows pinned GDLE, including discrete online-time accrual and source-defined propagation quirks, by explicit owner direction (2026-10-07). Other allegiance behavior remains pinned ACE. NPC emote behavior remains pinned ACE, including authored timing and source-defined durable stages. Rare structure follows the project owner's supplied Turbine evidence; uncertain probabilities, tier weights and timer policies require an explicit configured profile (docs/rare-evidence.toml). Compare both sources, document disagreements and retain authority hardening; this exception does not silently replace XP, treasure or unrelated formulas. `docs/network-coverage.toml` records source coverage, and `docs/divergences.toml` preserves user-supplied observations from other revisions with their uncertainty.

The initial deployment targets Windows, Linux and macOS with a lightweight host-console supervisor, one game child, and PostgreSQL for community shards. Initial playable acceptance means two stock clients can create fresh characters, enter the world, traverse outdoor and dungeon geometry, cross portals, observe each other, save/reconnect, and observe a newly published weenie without restarting. Existing player migration, complete combat/magic/AI and distributed simulation are later milestones.

**BASE-02:** Import the complete Patches world release once. It already includes the base world. The pinned complete release is v0.9.295, with base v0.8.8. Do not import the complete release as an incremental overlay or recursively execute every source SQL file. Upstream compilation has an explicit category order.

**BASE-03:** User-supplied DAT files are required for authentic collision/motion. Their hashes and supported versions MUST be verified. DATs MUST NOT be committed. Synthetic collision tests cannot establish DAT compatibility or stock-client playability.

## Crate ownership

Every row names separate packages, not modules of a monolithic server. The complete path inventory and dependency allowlists live in `architecture.toml`.

| Group | Crate | Sole responsibility |
|---|---|---|
| Foundation | `bace-random` | Versioned keyed event randomness and isolated character rare streams |
| Foundation | `bace-types` | IDs, property IDs and shared numeric primitives |
| Foundation | `bace-geometry` | Geometric primitives, frames, bounds and shapes |
| Foundation | `bace-config` | TOML configuration and validation |
| Foundation | `bace-observability` | Logging, metrics and diagnostic interfaces |
| Assets | `bace-dat` | DAT decoding, asset validation and immutable access |
| Assets | `bace-dat-service` | Legacy DAT distribution service |
| Assets | `bace-asset-overlay` | Pure, checked application of native patches to decoded DAT assets |
| Content | `bace-content` | Definitions, validation, catalogs and revisions |
| Content | `bace-content-tools` | Native TOML authoring, binary compilation and export |
| Content | `bace-import` | Legacy SQL/JSON, EmoteScript and official-world conversion |
| Network | `bace-wire` | Explicit legacy wire codecs, checksums and identifiers |
| Network | `bace-transport` | UDP, reliability, fragmentation, bounded peer state |
| Network | `bace-auth` | Authentication and access policy |
| Network | `bace-session` | Handshake/lifecycle and decoded action dispatch |
| Network | `bace-replication` | Authoritative projections to ordered wire output |
| Simulation | `bace-entity` | Runtime entity state and read projections |
| Simulation | `bace-gameplay-api` | Typed commands/effects and subsystem contracts |
| Simulation | `bace-motion` | AC locomotion, animation and motion-table behavior |
| Simulation | `bace-physics` | Integration, collision and accepted physical state |
| Simulation | `bace-world` | Entity mutation ownership, cells, landblocks, spatial state and geometry-gated loading-player insertion |
| Simulation | `bace-simulation` | Tick phases and orchestration of gameplay systems |
| Gameplay | `bace-character` | Creation, progression, attributes, skills and vitals |
| Gameplay | `bace-combat` | Attacks, damage, defenses and PvP rules |
| Gameplay | `bace-magic` | Casting, spells, enchantments and effects |
| Gameplay | `bace-ai` | Awareness, behavior, navigation intent and pets |
| Gameplay | `bace-inventory` | Containers, equipment, stacks and item ownership |
| Gameplay | `bace-economy` | Vendors, currency and player trades |
| Gameplay | `bace-crafting` | Recipes, salvage and crafting mutations |
| Gameplay | `bace-loot` | Treasure profiles and generated items |
| Gameplay | `bace-spawning` | Generators, spawning, despawning and decay |
| Gameplay | `bace-quests` | Quest state, counters and contracts |
| Gameplay | `bace-emotes` | Content-defined action sequences and scripts |
| Gameplay | `bace-social` | Friends, chat, channels and squelch |
| Gameplay | `bace-fellowship` | Fellowship membership and shared rewards |
| Gameplay | `bace-allegiance` | Allegiance hierarchy and rules |
| Gameplay | `bace-housing` | Houses, rent, access, hooks and storage |
| Gameplay | `bace-interactions` | Doors, locks, switches, portals, lifestones, books |
| Gameplay | `bace-world-events` | Scheduled/content-controlled world events |
| Gameplay | `bace-activities` | Chess and minigames |
| Storage | `bace-persistence` | Save coordination, repository ports and durable results |
| Storage | `bace-storage-codec` | Frozen binary schemas, envelopes and migrations |
| Storage | `bace-db-postgres` | Production SQL, migrations, pools and transactions |
| Application | `bace-admin` | Administrative command policy and handling |
| Application | `bace-tooling` | Maintenance workflow orchestration |
| Application | `bace-content-studio` | Desktop content authoring, offline candidate packs, DAT and landblock inspection |
| Application | `bace-runtime` | Composition, workers, startup and shutdown |
| Executables | `bace-server`, `bace-cli`, `bace-content-gui` | Minimal executable adapters only |
| Verification | `bace-compat`, `xtask` | Official oracle/replay; architecture and size checks |

**DEP-01:** Crate dependencies MUST be acyclic and explicitly allowed. Gameplay siblings MUST NOT depend on one another. They exchange typed commands/effects via `bace-gameplay-api`; `bace-simulation` sequences execution. Prefer narrow typed interfaces over generic event buses.

**DEP-02:** Foundation/physics/domain crates MUST NOT depend on sockets, SQL adapters or application composition. Replication is the only world-to-wire projection. Game systems MUST NOT write packet bytes or SQL.

**DEP-03:** `bace-server` MUST remain a thin adapter. `bace-runtime` owns service composition but MUST NOT absorb gameplay. `bace-tooling` owns CLI workflow implementation. Crate roots MUST remain declarations/re-exports; only `main` may minimally delegate. AST checks enforce the mechanical portion.

**SIZE-01:** At 1,000 physical lines every Rust file MUST have a concrete split plan. Above 1,500 it MUST be split before merge. Comments, blanks, tests and generated tracked Rust count. Splits MUST be by responsibility, not mechanical chunks. All workspace Rust is checked, including untracked source during development; build/reference directories are excluded.

## Protocol and session contract

**WIRE-01:** Preserve numeric IDs, widths, little-endian representation, flags, optional-field order, padding, string encodings, packed integers, checksums/ISAAC, fragment boundaries, counters, ordering, retransmission and valid session transitions. Database serialization MUST NOT be used as a wire codec.

Upstream's packet header is 20 bytes; the fragment header is 16 bytes. The outgoing body budget is 464 bytes (484 bytes including packet header); fragment data budget is 448 bytes. These are distinct from inbound allocation budgets. Windows-1252, Hash32 trailing-byte placement and ISAAC consumption are protocol details, not interchangeable library defaults.

**WIRE-02:** Every implemented feature MUST identify its pinned official source and have golden or differential fixtures. Tests MUST control clocks, IDs and random seeds when comparing bytes. Round-trip tests alone do not prove parity. Full compatibility is tracked feature-by-feature.

**WIRE-03:** Parsers/reassembly MUST bound memory, lengths, counts, sequence windows and lifetimes. They MUST reject malformed input without panicking or partially applying world mutations. Reliable-output overload MUST disconnect an unrecoverable peer rather than silently lose required messages.

**WIRE-04:** Authority/security hardening may change rejection and correction behavior but MUST preserve valid wire structures. Each intentional difference MUST be documented and tested. Do not replicate crashes, unchecked allocations or impossible movement as compatibility requirements.

## Simulation and authority

**SIM-01:** Physics and world mutation MUST share one dedicated operating-system thread at 30 Hz, owning the same entity state. Do not create a second physics state copy, a physics/world handoff each tick, or one task per entity. Network, content and database adapters execute separately and submit bounded inputs. Landblocks/cells are logical ownership units; distributed ownership and an ECS framework are deferred.

Tick input is server-ordered typed commands, validated immutable assets/content, explicit time and controlled random state. Tick phases are input validation, legal motion intent, physics, gameplay effects, state commit, replication projections and dirty-save scheduling. Output is typed events/snapshots. No network-originated command has a direct accepted-pose setter.

**SIM-02:** Physics MUST produce accepted pose, velocity, ground contact and collision events for all actors. Client position, contact, jump vectors and epochs are untrusted. Server teleports use explicit privileged commands and invalidate stale movement epochs.

**SIM-03:** Movement MUST sweep through AC terrain, object, BSP/cell and portal geometry. Missing geometry blocks entry. Jump impulses MUST derive from authoritative capabilities, stamina, burden and legal stance. Endpoint-only checks are insufficient.

**SIM-04:** Reconciliation MUST validate reported positions against accepted history and legal motion, accounting for stock-client omitted key-release transitions. A report may help infer a legal stop and resimulation; it MUST NOT directly install a pose, create ground contact or grant extra movement energy. Tolerances require stock-client trace evidence.

**SIM-05:** Physics/tick functions MUST NOT call wall clocks, perform I/O, query databases, block on channels, await or send packets. Effects are typed outputs. Actor identity MUST have exactly one owner across transfers.

AC-specific collision/motion is ported with upstream formulas and precision until measured tests justify change. Synthetic physics foundations are explicitly separate from validated AC collision. Same-build replay determinism is required; cross-platform bitwise determinism is not assumed.

**MATH-01:** SIMD optimization MUST retain a scalar reference and pass differential collision/movement traces before becoming the default. Retail x87 precision and intermediate rounding MUST NOT be assumed equivalent to SIMD `f32` or `f64`. Preserve operation order and explicitly assess rounding, denormals, FMA and boundary decisions. Do not globally enable fast-math or host-specific CPU instructions in distributed builds. Provide a portable baseline and gate optional CPU features correctly.

**COST-01:** Keep the initial deployment to one lightweight host supervisor, one game child and PostgreSQL. Tick work MUST use bounded batches and reusable storage, without per-entity task scheduling or database reads. Add SIMD, threads, dependencies or infrastructure only with a measured bottleneck and a documented improvement in representative end-to-end workloads; include memory and synchronization costs. A microbenchmark alone cannot establish stock-client capacity. Detailed math gates are in `docs/physics-math.md`.

## Content and storage

Native flow is `TOML -> typed validation -> versioned binary -> PostgreSQL publication journal -> immutable .bace generation`. Legacy SQL and JSON enter only through conversion tools. Native runtime content and saves MUST NOT use JSON/JSONB. Runtime content MUST use checked mapped views and bounded prepared assets; startup MUST NOT decode or retain the whole world. Complete world compilation and bounded mapped-region preparation exist; gameplay adoption and authentic geometry admission remain separate integration gates.

Pinned ACE treasure lookup rows, enum/spell indexes and mutation scripts use
native TOML owned by `bace-loot` and a frozen DTO owned by `bace-content`.
`bace-content-tools` compiles that source into namespace 52 of the aggregate `.bace` generation,
and prepared by `bace-loot` at cold startup. `bace-runtime` selects the accepted
set and fails closed if it is missing. Table-set replacement requires a graceful
game-child restart; native nested loot graphs and rare profiles retain their
separate namespaces 46–47 and publication path. Retail PCAP comparison limits
are documented in `docs/retail-loot-gap-analysis.md`.

**PACK-01:** Compiled world/content MUST use one immutable aggregate `.bace` base and at most two active delta packs (one to three active `.bace` files total), never a file per object or landblock. Temporary replacement and retired reader-pinned files may coexist during safe publication/compaction. PostgreSQL owns the accepted manifest pointer. Files and manifest MUST be durable before acceptance; restart opens that exact generation. Old mapped generations remain alive while readers retain them. Never rewrite or truncate a mapped backing file.

**PACK-02:** One dedicated blocking pack-I/O OS thread MUST own writes, flushes, compaction and reclamation. Asset reads/preparation MUST have separate bounded capacity; simulation and persistence MUST NOT await pack I/O. Maintenance MUST yield to save pressure. Codec/compiler APIs alone do not establish this runtime composition.

**PACK-03:** Mapping is the sole permitted unsafe boundary, private to the storage codec. All binary parsing MUST use checked offsets and bounded lengths. Platform-specific crash durability and deployment immutability requirements are documented in `docs/pack-format.md`.

**STORE-01:** Use indexed scalar identity/name/type/revision columns plus binary aggregates. Ownership, uniqueness, container relationships and atomic transfers remain relational. No property-row ORM graph traversal. PostgreSQL is the sole production backend initially.

**STORE-02:** Binary payloads MUST use frozen application-versioned DTOs with magic, version, payload length, integrity checksum and bounded decoding. Postcard format stability does not make arbitrary application struct changes compatible. Preserve numeric widths, absent/zero distinctions, unknown property IDs and ordered collections. Schema changes require explicit migrations. Unsupported schema versions fail explicitly.

**STORE-03:** TOML tooling MUST support lossless checked conversion. Unsupported/nonrepresentable legacy data MUST be reported, never discarded. SQL import executes only in an isolated disposable MariaDB environment: the official dump includes database recreation statements. Production PostgreSQL and existing ACE databases MUST NOT receive those statements. Running the game MUST NOT require MariaDB.

**HOT-01:** Native SQL inserts and tool publication MUST share immutable candidate revisions and a transactional trigger-generated journal. Persist accepted heads and retain previous revisions; rejection MUST survive restart without destroying last-known-good content.

**HOT-02:** Validation and secondary-index construction occur off the world thread. Accept complete batches, then atomically publish catalog generations at tick boundaries. Live instances retain their template revision; new spawns resolve current accepted content. Template insertion does not itself spawn an instance.

**HOT-03:** Rejected batches MUST be quarantined with diagnostics and MUST NOT block independent future batches. Corrections are new revisions. Journal order MUST be commit-safe; a sequence/bigserial allocation alone is not commit ordering. LISTEN/NOTIFY is a wakeup only; polling, restart and disconnect recovery use durable state.

**HOT-04:** Valid committed templates MUST become available for spawning within two seconds under healthy normal load. Observe commit-to-activation latency. Invalid or incomplete batches never become visible.

Operator inbox review belongs to `bace-admin` (authenticated UI and explicit
confirmation), `bace-runtime` (bounded scan, typed validation, pack publication),
`bace-content-tools` (native TOML and frozen codecs), and `bace-db-postgres`
(commit-ordered mixed candidate journal and accepted manifest CAS). Source files
are never scanned automatically. A preview is fenced to the accepted manifest
and exact file bytes; additions, replacements and tombstones preserve unrelated
accepted records. Derived world indexes publish in the same immutable delta.

## Save coordination and failure behavior

**SAVE-01:** Persistent mutations increment an aggregate revision. A bounded save coordinator schedules routine dirty state every five seconds and coalesces queued snapshots. A completion for revision R MUST NOT clear a newer mutation. A stale routine snapshot MUST NOT overwrite a newer transaction.

**SAVE-02:** Distinguish in-memory mutation revisions from durable database CAS versions. One ordered write sequencer initially serializes overlapping writes. Valuable operations reserve participants, wait for preceding writes, commit all affected rows and ownership links atomically with CAS checks, then release authoritative success. Unrelated simulation continues.

**SAVE-03:** Valuable operations have durable operation IDs recorded in the same transaction. An uncertain commit acknowledgment MUST be resolved using that ID before retrying. CAS conflicts roll back the entire operation; do not blindly retry stale proposed state.

**SAVE-04:** Failures retain dirty state, expose unhealthy persistence and bound retry/queue growth. New valuable operations cannot succeed without durability. Logout and graceful shutdown drain outstanding work or report failure. Five seconds is a scheduling cadence, not a promised maximum crash-loss interval during database failure.

**SAVE-05 — No starvation:** Routine saves MUST preserve their first-dirty time across coalescing and failures, and select overdue aggregates by age/deadline rather than object ID. A dedicated persistence OS thread with a current-thread I/O runtime MUST have reserved database capacity. Online and offline writes share its ownership fencing and bounded fair scheduling; offline characters MUST NOT be physics-ticked. Continuous inbound updates, hot entities, critical operations, reads and content publication MUST NOT indefinitely defer a due save. Bound work per scheduling turn and critical-operation bursts; check due work before accepting another burst. Exhausted capacity MUST propagate backpressure while retaining unsaved state. Tests MUST continuously inject competing work and prove every due aggregate receives service. Expose oldest dirty age, deadline misses, queue capacity and write latency; database outages are visible durability failures, never silent clean acknowledgments.

## Host console and lifecycle

**HOST-01:** BetterACE MUST offer a default authenticated loopback web console in a parent supervisor independent of the game child. Host credentials MUST remain separate from player accounts. Logs, errors and child lifecycle status MUST be bounded and available during child failure.

**HOST-02:** Restart MUST drain through the owning services and acknowledge durability before exit. A failed drain MUST remain blocked with a retry of the same operation; no force-kill of a dirty child or duplicate world owner is permitted. Player character views and trading/auction features are later capabilities with separate authorization.

Networking MUST own a dedicated I/O thread; simulation owns one plain OS thread; persistence owns its own current-thread runtime; pack writing owns a plain blocking thread. Tokio is an I/O implementation detail, not a dependency of physics or gameplay.

## Verification and delivery gates

CI MUST enforce root syntax, file lengths/split plans, dependency allowlists, formatting, clippy and tests. Dedicated jobs exercise disposable PostgreSQL, official oracle fixtures and optional user-supplied DAT/client scenarios. Missing external prerequisites MUST be explicit; skipped checks are not passes.

Milestones:

1. **A: governance/scaffold:** all inventoried crates, ownership inventory, executable checks and compiling workspace.
2. **B: foundations:** exact wire/transport/session contracts, authentication, validated DAT assets, native binary content, imports, PostgreSQL and live publication.
3. **C: playable slice:** two stock clients create/select characters, enter outdoor/dungeon areas, move/collide/jump/portal, observe each other, persist and observe live spawned content.
4. **D: failure/performance:** packet loss/reordering/wrap/flood, forged movement, invalid content/reconnect, stale-save/CAS/uncertain-commit tests and measured budgets.

Initial benchmark target is 100 moving synthetic players and 1,000 non-player physics bodies for ten minutes, p99 tick computation <=25 ms with no sustained missed 30 Hz deadlines. Injecting 500 ms database latency MUST NOT stall ticking. Synthetic-load results MUST NOT be described as full stock-client capacity. Record hardware/build/dataset.

Compare database size, aggregate query count, loading, CPU, save latency and memory against equivalent ACE data and durability. A smaller table count alone is not evidence of lower total cost. Never claim full parity, playability or performance without the corresponding evidence.

Character feature ownership: `bace-character` owns skill transitions, derived skill
values, taboo/name policy and per-character UI validation. `bace-crafting` owns
immutable tinker/salvage decisions; per owner direction, salvage yield follows the
pinned GDLE formula and imbue caps are exactly 0.33/0.38 while retaining skill
scaling. `bace-simulation` reserves the authoritative character, inventory and
registry owners; `bace-runtime` freezes one complete revision and correlates durable
receipts. Frozen schema3 supplements belong to `bace-storage-codec`; relational
identity, schema downgrade protection and atomic receipts belong to PostgreSQL.
Prepared combat/magic projections consume specialization values without holding
another mutable character aggregate. No successful valuable-operation packet may
be projected from a proposed or uncertain result.

Generator feature ownership: `bace-spawning` owns source profile selection,
timers, registries and lifecycle transitions; its typed contracts live in
`bace-gameplay-api`. `bace-loot` owns source treasure and equipment decisions,
and `bace-economy` owns retained shop stock. `bace-runtime` prepares bounded
immutable content/DAT closures; `bace-simulation` admits them into the existing
world, inventory, combat and magic owners. No generator owns duplicate physical
state or performs tick-time I/O. Transient populations rebuild on restart.
Acquisition and durable retirement use `bace-persistence::WorldPlacementOperation`
with the world epoch included in PostgreSQL's atomic idempotency fingerprint;
only an exact durable receipt releases the simulation reservation.

Character-start content belongs to `bace-content`: a frozen versioned profile
preserves authored starter-gear/spell order and starting-area spell references.
`bace-content-tools` compiles native TOML to integrity-checked immutable `.bace`
bytes. The pinned built-in profile is an immutable bootstrap default, not a live
catalog mutation. `bace-runtime` joins it to the accepted pack and verified DAT
assets off-thread; `bace-character` alone interprets creation and starter rules.
Missing referenced templates or geometry reject creation before persistence.
