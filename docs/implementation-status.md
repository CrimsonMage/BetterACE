# Implementation status

This file records verified capability, not the architecture's intended outcome.

| Milestone | Status |
|---|---|
| A: workspace and governance | Implemented: inventoried workspace, thin roots, dependency/size checks and CI |
| B: protocol, assets, storage and content foundations | In progress |
| C: two stock clients in an authoritative world | Not implemented |
| D: full failure/performance acceptance | Not verified |

The crate inventory uses `scaffolded`, `foundation`, and `implemented` to distinguish declared ownership, partial working foundations, and a completed stated crate scope. Protocol compatibility is assessed separately; no whole-server parity is claimed.

## Working foundations

| Area | Implemented evidence | Remaining boundary |
|---|---|---|
| Protocol | Pinned official C# golden vectors; full named opcode catalogs, character/property/DDD/movement/object/social codecs, bounded peer driver and paired UDP worker | Complete production lifecycle composition, remaining payload/action families, full replication and stock-client transcript qualification |
| Accounts | Bounded Argon2id workers; canonical PostgreSQL accounts; race-safe optional auto-creation (default on); generation-fenced account/session drain | Complete stock-client character flow, bans and production service composition |
| Assets | Bounded archive/outdoor readers plus full XPTable, SkillTable and CharGen readers; real portal tables prepared for 38 skills and 13 heritages | Motion tables, setup/animation assets, cell/BSP/object/portal collision |
| Physics/world | Single-owner synthetic fixed-step motion, finite input checks, speed/jump bounds, swept obstacles, explicit teleport epochs and placement validation | Authentic AC motion/physics and stock-client reconciliation; no SIMD conversion is qualified yet |
| Runtime | Dedicated simulation owner; bounded command/result delivery and kernel recovery; tested UDP-to-progression-to-primary-reply path; separate content/save adapters | Composed gameplay lifecycle, accepted/rejected action routing, projections, asset loading and save integration |
| Content | Typed full weenie property families; native TOML/binary round trips; immutable revisioned catalogs and indexes | Runtime instantiation/spawning and system-specific interpretation |
| Content Studio | egui native weenie editor with 21 property families, undo/redo, named property/spell lookup, safe TOML saves, legacy import/export bundles and incremental TOML-to-mapped-pack builds | EmoteScript language, death-treasure profiles, gameplay calculators, saved preferences, live pack activation and non-Linux SQL staging; Windows/macOS validation outstanding |
| Imports | Checked ACE Entity/Lifestoned JSON; isolated MariaDB execution of unchanged weenie SQL, all 24 weenie/property tables, count/hash manifest | Non-weenie world tables; complete official-world dump conversion explicitly fails |
| Storage | Binary aggregate PostgreSQL schema, migrations, revision CAS, atomic/idempotent operations, durable candidate journal and quarantine | Frozen player/inventory/gameplay save schemas and their mutation integration |
| Live content | Direct candidate insert and tooling share a trigger journal; off-thread validation, durable accepted heads, atomic bounded generation delivery, restart recovery | Game-world adoption/new-instance spawning; harness activation is not gameplay |
| Save fairness | Dedicated OS thread/current-thread runtime, reserved writer connection, dirty-age preservation, bounded critical bursts and alternating routine/offline queues | Composition with gameplay ownership and logout; production health export/load qualification |
| Mapped packs | Streaming immutable `.bace` compiler, bounded authenticated indexes, lazy payload integrity checks, base/delta lookup, tombstones, restartable manifests; PostgreSQL accepted-generation transaction | Full-world compiler, dedicated pack-I/O/asset workers and replacement of the in-memory content harness remain uncomposed |
| Offline ownership | PostgreSQL epoch fencing, login/logout CAS and idempotent offline cached-XP receipts | Full allegiance calculation, online XP routing, restart lease takeover and owned multi-character transactions |
| Host console | Authenticated loopback dashboard, bounded logs and reusable supervisor/child drain protocol | Gameplay drain composition; Windows private-ACL provisioning and platform qualification |
| Clothing | Initial native clothing DTO/merge/palette-selection foundations | CustomClothingBase importer, DAT composition and observer appearance refresh are not integrated or qualified |

`bace-server serve` deliberately returns an error without opening a game socket. `check`, `exercise`, and `content-worker` are usable foundation tools. The default host console manages a foundation child that explicitly reports gameplay as unavailable. Character progression/creation allocation, training/specialization and quest predicates now have source-backed foundations; most gameplay remains scaffolded. Replication sequencing/knowledge and bounded DDD preparation are foundations, not complete world integration. See `host-console.md`, `pack-format.md` and `persistence.md` for foundation boundaries.

## Next implementation sequence

1. Port and oracle-test AC motion tables, setup/cell/BSP/portal assets and authoritative movement. Retain scalar reference arithmetic; qualify SIMD only after profiling and the gates in `physics-math.md`.
2. Complete reliable UDP/session orchestration and stock-client login, character creation/selection and world-entry messages, with pinned transcript fixtures.
3. Implement character/spawning/interaction/replication slices and integrate catalog generation swaps on the simulation owner. Add frozen character save DTOs, dirty tracking, valuable-operation reservation and bounded shutdown drain.
4. Convert the remaining official-world systems before accepting the complete pinned dump. Run two actual stock clients through outdoors, dungeon, jump, portal, nearby-player visibility, reconnect and live new-weenie spawn scenarios.
5. Run paced ten-minute mixed-load and failure acceptance, including delayed/failed saves, packet loss/reordering, malformed packets, forged movement and late content. Compare measured memory, CPU, storage size and query/save costs with equivalent ACE workloads.

These remain implementation work, not merely missing test runs. Current synthetic benchmarks do not establish these milestones. See `validation.md` for the checks actually performed.


## Networking expansion evidence

`docs/network-coverage.toml` inventories immutable official network sources and
compiled codec dependencies with SHA-256/Git-blob provenance, ownership and
explicit unsupported/foundation statuses. `cargo xtask check` checks its structure;
`tools/bace-compat/oracle/network_inventory.py --check --source <pin-directory>`
checks coverage against the official Git tree. Identifier coverage is separate
from payload, dispatch, gameplay and stock-client compatibility.

The independent C# oracles now cover named catalogs, selected messages, movement
conditional layouts and queue/retransmit behavior. Character and quest harnesses
exercise thousands of synthetic scenarios using verbatim pinned methods. Runtime
loopback tests cover handshake recovery, pre-cookie rejection, reliable delivery,
duplicate-account drain, bounded authentication and DAT-worker result retention.
None of these tests establishes a playable shard.

The requested full networking/full gameplay scope is **not complete**. Remaining
work includes remaining appraisal/combat/magic payload families and complete gameplay-backed object/social handling,
remaining action handlers, production character/save schemas, authoritative AC
asset/world composition, complete replication and all gameplay-owner milestones
in `docs/gameplay-parity.toml`. `serve` remains gated. The supplied divergence
register is tracked in `docs/divergences.toml`; reports from another ACE revision
are leads to validate, not automatic changes to BACE's pinned authority.


## BetterACE continuation

The repository/project display name is now **BetterACE**. The `bace-` package and
executable prefixes, `BACE_*` environment variables and `.bace` format remain stable.

Full object creation/update/appearance/physics/game-data codecs and Turbine/social
codecs now have pinned C# vectors. XP, skill and character-generation DAT tables
have independent decoders and actual-asset validation. Character training and
specialization preserve authoritative metadata and reject unsupported augmentations.
The primary progression request/result path is connected in a real UDP integration
test with a synthetic world. It does not establish stock-client world readiness or
full rank-up side effects, persistence, geometry, object visibility or gameplay parity.


Inventory/trade/vendor wire requests and event/listing payloads now have explicit
bounded codecs and pinned fixtures. They do not implement item ownership, trade
transactions, vendor pricing or durable success policy. Turbine framing preserves
the pinned +8 length arithmetic and ByName response dispatch; oversized lossy
UTF-16 prefixes are rejected with documented hardening tests.
