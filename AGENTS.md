# Mandatory engineering rules

Read `ARCHITECTURE.md`, `architecture.toml`, and the target crate's README before changing code. The user's explicit requirements take precedence over repository guidance. MUST and MUST NOT are requirements, not suggestions.

## Project identity

- **NAME-01:** The project MUST be named **BetterACE**, expanded **BetterACEmulator**. The repository name is **BetterACE**. Existing compatibility identifiers are retained: workspace packages and executables MUST use the `bace-` prefix (except `xtask`). Runtime content files MUST retain the `.bace` extension. Official ACEmulator/ACE source names, protocol identifiers and provenance MUST NOT be renamed as branding.

- **LICENSE-01:** BetterACE MUST use `AGPL-3.0-only`. Every workspace crate MUST inherit `license.workspace = true` from the root manifest. Contributors MUST preserve the full license text, upstream copyright notices and source attribution. Any license change requires an explicit project-owner decision.

## Ownership and boundaries

- **OWN-01:** Every change MUST have one owning crate. Crates MUST obey the direct-dependency allowlist in `architecture.toml`. Gameplay siblings MUST communicate through `bace-gameplay-api`, not depend on each other.
- **OWN-02:** `bace-server` MUST remain a thin executable adapter. Service construction, lifecycle and workers belong in `bace-runtime`; gameplay belongs in its named subsystem, never in either executable or runtime.
- **OWN-03:** Shared crates MUST NOT become miscellaneous utility collections. New systems MUST be assigned explicit ownership in the inventory before implementation.
- **OWN-04:** Parallel agents MUST have disjoint file ownership. Coordinate shared contract changes with the integrating agent. Do not overwrite another agent's work.

## Thin roots and file size

- **FILE-01:** Every `lib.rs` MUST contain only documentation, attributes, external module declarations, imports and re-exports. No implementation, inline modules, constants, types or inline tests.
- **FILE-02:** Every `main.rs` MUST follow the same rule, except for one minimal `main` function that delegates to a named entry module or application crate. No handlers, configuration parsing, construction, branching or business logic.
- **FILE-03:** At **1,000 physical lines**, a Rust file MUST have a concrete entry in `docs/split-plans.toml` identifying ownership, responsibility boundaries and destination modules. Plan the split before growing further.
- **FILE-04:** A Rust file above **1,500 physical lines MUST be split before merge**. Count blank lines and comments. Tests and generated tracked Rust are included. Do not use minification, meaningless fragments or `include!` to bypass this limit.
- **FILE-05:** Tests MUST live in named modules or integration-test files, not inline in crate roots.

## Authority and compatibility

- **WIRE-01:** Protocol behavior MUST derive from official ACEmulator/ACE at the pin in `docs/baselines.toml`. Fork changes MUST NOT become compatibility requirements.
- **WIRE-02:** Implemented codecs MUST preserve field sizes, endianness, IDs, padding, conditional fields and encoding. Transport MUST preserve sequencing, checksums, fragmentation and valid-session behavior.
- **WIRE-03:** Every compatibility claim MUST cite source provenance and a golden/differential test. A self round-trip is insufficient evidence of ACE parity.
- **WIRE-04:** Untrusted lengths, sequences and counts MUST be bounded. Malformed input MUST return an error, not panic or partially mutate world state.
- **AUTH-01:** Client pose, velocity, jump, contact and timestamps MUST remain untrusted. Only authoritative physics may produce accepted physical state. Server teleport MUST be explicit.
- **AUTH-02:** Swept collision and valid geometry MUST gate movement. Missing assets MUST block entry rather than disable collision.
- **AUTH-03:** Physics/tick code MUST NOT perform I/O, wait on async operations, send packets or obtain wall-clock time. Time and immutable inputs MUST be explicit.
- **AUTH-04:** Hardening deviations MUST be documented with valid-client and invalid-input regression tests. Do not claim a playable stock-client movement model before testing it.
- **AUTH-05:** Physics and world state MUST have one dedicated simulation-thread owner. Adapters MUST use bounded inputs; no per-entity tasks or duplicate mutable physics/world state.
- **MATH-01:** SIMD changes MUST retain a scalar reference, explicit precision/CPU assumptions, boundary regressions and representative performance evidence. x87-to-SIMD numeric equivalence MUST NOT be presumed. Global fast-math and deployment-wide native-CPU assumptions are forbidden.
- **COST-01:** Optimizations and added infrastructure MUST justify CPU, memory and synchronization cost. Keep runtime work bounded and avoid per-tick allocation where reusable buffers suffice.

## Data and durability

- **DATA-01:** Native authoring MUST use TOML; runtime content/saves MUST use versioned binary aggregates and relational identity/constraint columns. JSON is permitted only for legacy conversion, tooling metadata and oracle inputs, not native content or saves.
- **DATA-02:** Binary storage MUST use frozen versioned DTOs with explicit migrations, decode limits and integrity checks. Do not persist evolving gameplay structs directly.
- **DATA-03:** All production SQL/connections MUST belong to `bace-db-postgres`. Live native SQL content insertion MUST enter the same durable candidate-publication journal as tooling.
- **DATA-04:** Catalog publication MUST be validated and atomic across indexes. Rejected content MUST preserve persisted last-known-good heads. Notifications alone are not a durable change feed.
- **SAVE-01:** Routine state MUST use dirty revisions and a five-second scheduling cadence. Completion of an older save MUST NOT clear newer changes or overwrite newer durable state.
- **SAVE-02:** Valuable operations MUST commit atomically with revision checks and durable operation IDs before success is reported. Uncertain commit outcomes MUST be resolved before retrying.
- **SAVE-03:** Queues MUST be bounded. Failures MUST retain dirty state and surface degraded health. Graceful shutdown MUST drain saves; never describe five seconds as a guaranteed crash-loss bound during outages.
- **SAVE-04:** Saves MUST NOT be starved. Coalescing MUST preserve the original dirty age. Scheduling MUST be deadline/age ordered, reserve persistence capacity, and bound bursts of other work. Hot entities, new updates, critical operations, content imports, and read traffic MUST NOT indefinitely postpone a due save. Overload MUST apply backpressure with unsaved state retained; missed deadlines MUST be observable.

## Runtime packs and hosting

- **PACK-01:** Compiled runtime content MUST use immutable `.bace` files. Startup MUST reopen the accepted generation without rebuilding or decoding the whole world. Live changes MUST use small immutable segments; never modify or truncate a mapped backing file.
- **PACK-02:** A dedicated blocking pack-I/O thread MUST own runtime pack writes, flushes, compaction and reclamation. Asset preparation MUST have separate bounded capacity. Simulation MUST NOT wait for file operations. Maintenance MUST yield to save pressure and use bounded buffers/queues.
- **PACK-03:** Only the reviewed private mapping boundary in `bace-storage-codec` may contain unsafe code. Its crate MUST retain `unsafe_code = "deny"` with one explicit mapping exception; every other crate MUST inherit workspace `forbid`. Parsing and record views MUST remain safe checked-offset code.
- **HOST-01:** The default authenticated host console MUST remain available independently of the game child. Host privileges MUST be separate from game accounts. Restart MUST retain unsaved state and report blocked durability; it MUST NOT force-kill a dirty child or start a duplicate world owner.
- **PORT-01:** Runtime packs, native content tools and host/server interfaces MUST target Windows, Linux and macOS. Optional legacy SQL staging prerequisites MUST be labelled honestly; no platform support claim without corresponding validation.

## Validation and honesty

- **QA-01:** Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cargo xtask check`. Run relevant database/asset/oracle integration suites when those paths change.
- **QA-02:** New boundary/schema/compatibility changes MUST update the architecture inventory, provenance and appropriate tests. Do not manufacture passing tests that simply reproduce implementation logic.
- **QA-03:** Deferred features MUST be labelled scaffolded or unsupported. No production `todo!()`/`unimplemented!()`, fabricated success, silent data loss, ignored failing tests or unsupported claims of parity/performance.
- **QA-04:** Preserve source attribution and licensing. Proprietary DAT assets, real credentials and player captures MUST NOT be committed.

Mechanical enforcement is in `cargo xtask check` and CI. Authority, durability and compatibility also require subsystem tests and review; a passing structural check is not a gameplay proof.
