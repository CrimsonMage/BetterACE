# Foundation validation

Recorded 2026-10-07. These checks establish the implemented foundation's contracts, not whole-server parity or playability. No existing database was modified; integration suites created disposable local clusters/instances.

## Environment

- Rust/Cargo 1.96.1 through the complete `+stable` installation; the named pin lacks some local binaries (see README).
- Linux 7.2.9-1-cachyos, x86_64, Intel Core i7-14700F, reported CPU governor `powersave`.
- PostgreSQL 18.6, private MariaDB 13.0.2, .NET SDK 10.0.302.
- Official source/data pins and local DAT hashes are in `baselines.toml`. Proprietary DATs and private database files are ignored and not included in this repository.

## Checks performed

```sh
cargo +stable fmt --all -- --check
cargo +stable clippy --locked --workspace --all-targets -- -D warnings
cargo +stable test --locked --workspace
cargo +stable xtask check
BACE_MARIADB_BASEDIR="$PWD/.local/mariadb/usr" cargo +stable test --locked -p bace-import --test mariadb -- --ignored
BACE_DAT_DIRECTORY=/home/jokerfactor/Documents/GitHub/Turbine/DATS cargo +stable test --locked -p bace-dat --test archive -- --ignored --nocapture
```

Formatting, Clippy and architecture checks passed. The final workspace run passed **130 tests**, with zero failures and four external-prerequisite tests ignored in that invocation. Those four passed in their explicit runs below. The architecture checker covers all 48 crates and 182 Rust files, root syntax, Cargo target aliases/examples/build scripts, dependency allowlists, module ownership/escape paths, includes and physical line counts. The largest Rust file is the pinned enum mapping `motion_names.rs`, 832 lines; no file has reached the split-planning threshold.

The default test suite includes real PostgreSQL CAS/idempotency, account uniqueness, durable content publication/quarantine/restart, reserved writer drain, bounded row-lock failure, starvation/fairness/backpressure and locked-worker shutdown. A separate adapter-isolation regression holds a save backend for at least 500 ms and until the dedicated synthetic simulation completes, then releases and drains the save; this tests thread isolation, not production mixed-load capacity. The four normally ignored external-prerequisite tests were also run separately: all three MariaDB tests and the actual DAT test passed. Local archives yielded 79,694 portal records, 805,348 cell records and 118 language records; actual outdoor landblocks were decoded too.

The pinned official C# oracle was regenerated in a temporary copy and matched the checked-in 66,449-byte fixture byte-for-byte, SHA256 `4def73f2eec5f007ff56a335ce3e524e0552aa06e8bf892955222e4425b57f26`. Its feature scope is listed in `../tools/bace-compat/README.md`; this is not evidence for unimplemented opcodes or session transcripts.

The BACE rename was checked across all 47 prefixed packages, imports, executable names, configuration examples, CI and the lockfile (`xtask` remains unprefixed). Original SQL migration bytes, upstream `ace_world` schema names and the official wire fixture were preserved. Both executable help commands and config validation pass. A disposable host-console process test provisioned credentials over stdin, rejected unauthenticated status access, signed in, restarted the foundation child through authenticated HTTP, observed a changed generation with `game_ready=false`, and shut down gracefully without a remaining child. This does not establish gameplay save-drain integration.

New foundation tests cover immutable `.bace` compilation, an independently generated format golden, lazy corruption detection, base/delta tombstones and restartable manifests; mapped-generation PostgreSQL acceptance; ownership fencing and idempotent offline cached XP; dedicated persistence-thread isolation and fairness; host authentication/CSRF/bounds; and blocked/retried child drain. xtask now also enforces the narrowly reviewed mmap unsafe boundary.

Earlier CLI smoke checks converted the all-property-family TOML fixture to binary and back, and converted unchanged official Arrow SQL plus complex SQL through a real private MariaDB instance into TOML/manifest directories. Temporary outputs were removed. Config validation and the finite synthetic worker also ran successfully.

## Synthetic measurement

```sh
cargo +stable run --release -p bace-server -- exercise --ticks 18000 --players 100 --bodies 1000
```

The final release run completed 18,000 fixed synthetic steps on the dedicated simulation thread. The 100-microsecond histogram placed p99 at **at most 100 microseconds**; maximum observed computation was **33.347 microseconds**. An earlier run before the final admission/shutdown changes had the same p99 bucket and a maximum of 115.02 microseconds. These were validation runs, not a controlled optimization comparison. No native-CPU or fast-math build flags were added. Measurements include bounded command intake and kernel computation; startup, scheduling sleep and external adapters are excluded.

This is ten minutes of **simulated** time executed unpaced, not a ten-minute paced endurance test. Actors eventually meet the synthetic wall; it is not a sustained authentic roaming workload. These measurements are not a SIMD comparison, an ACE resource-cost comparison or a production capacity result. Authentic DAT collision, UDP traffic, client correction, gameplay, save latency and mixed-load contention remain outside the measured workload. Full performance acceptance remains open.

## Not validated

No two-stock-client playable session, full official-world import, client movement/reconciliation oracle, production SIMD path, real gameplay persistence lifecycle, paced mixed-load endurance or measured resource advantage over ACE has been established. See `implementation-status.md` for the remaining implementation sequence.


## Networking expansion validation — 2026-10-07

The expanded workspace run passed **236 tests**, with zero failures and
5 external-prerequisite tests ignored in that invocation. The real DAT archive
suite and independent Python/system-zlib DDD suite were run explicitly and passed;
the three unchanged MariaDB import tests were not rerun for this networking change.
Default workspace tests include disposable PostgreSQL integration tests.

Commands used `RUSTUP_TOOLCHAIN=stable`, which is the installed Rust/Cargo 1.96.1;
the repository toolchain pin was not changed. Formatting check, workspace Clippy
with `-D warnings`, and `cargo xtask check` all passed. Structural checks cover
48 crates and 262 Rust files; the largest remains 832 physical lines.

All four expanded official oracles (messages/movement, transport, character and
quests) were regenerated against locally supplied source verified byte-for-byte
with immutable official GitHub files; their fixture SHA-256 values were unchanged.
The primitive oracle also ran successfully with new sequence-wrap vectors. Its
counter harness materializes the full C# sequence before slicing boundary values,
so LINQ cannot skip the side-effecting counter advances. The source inventory check
verified 458 entries against official Git blob IDs.

New runtime regressions include Linux paired UDP handshake/loss retry, no output
before cookie proof, reliable delivery, duplicate-account drain, stale completions,
blocked output/shutdown, authentication panic-result retention and cancelled
blocking-password work. DDD worker tests cover bounded job ownership, matching
metadata, stale generations, retry retention and drain accounting. Domain oracles
include 6,338 character/quest scenarios plus movement flag combinations and transport
traces. These are isolated tests, not a playable-stock-client, full gameplay,
whole-session differential or multi-platform qualification.


## BetterACE continuation validation — 2026-10-07

The latest full workspace run passed **308 tests**, with zero failures and nine
external-prerequisite tests ignored in that invocation. The two new actual-DAT
suites were run explicitly and passed: XP/Skill/CharGen decoding, then pure runtime
preparation of all 13 heritages and 38 skills. They verified the pinned portal hash;
no proprietary record bytes were added to fixtures. Other ignored external suites
were not counted as passes for this continuation.

Formatting, workspace Clippy with `-D warnings`, and `cargo xtask check` passed.
The current shared workspace has 50 crates and 320 Rust files; the largest remains
832 lines. The concurrently developed content GUI/pack changes were left intact;
the full workspace numbers include those crates, not only this networking work.

Expanded message and DAT oracles regenerated byte-identically, and the character
oracle regenerated reproducibly including the new training scenarios. Message
fixtures compile 203 full pinned C# sources with 20 additional verified provenance
sources; DAT fixtures compile 33 unchanged sources. The coverage inventory now
verifies 524 source/dependency entries against official Git blobs, with 288 marked
foundation and 236 unsupported. These source statuses do not assert full gameplay.

New runtime evidence covers UDP action → typed session dispatch → simulation-owned
progression → primary XP/trait projection → reliable UDP. Saturated output retains
results and commands while ticks continue. Recoverable exit and failed startup
return character ownership; report-only exit refuses to discard queued progression
or outcomes. Zero-XP actions retain their revision and still produce replies,
while replayed action sequences are rejected. Network-worker replacement cannot
reuse process-local session generations. These are synthetic-world integration
and source-backed codec/domain tests, not playable stock-client qualification.

GitHub repository rename to CrimsonMage/BetterACE and the local origin URL were
verified. Project metadata, agent guidance and host display branding use BetterACE;
crate names, environment variables, wire provenance and `.bace` formats stay stable.


## Whole-world, gameplay and GDLE authority expansion — 2026-10-07

Final workspace checks use the installed complete `+stable` toolchain, verified as
Rust/Cargo **1.96.1**, matching the repository pin. Formatting check, workspace
Clippy with `-D warnings`, workspace tests and `cargo xtask check` all passed.
The workspace run passed **434 tests**, with zero failures and **21 explicitly
ignored external-prerequisite tests**. Ignored tests are not counted as passes.
Architecture validation covers 50 crates and 464 Rust files; largest is 832 lines.

Separate targeted evidence completed during this implementation:

- Complete pinned Patches SQL imported unchanged through disposable MariaDB;
  table counts preserved for all 54 source tables. The aggregate includes 43,913
  weenies, 915,050 other typed source rows, 38,152 landblock indexes and 14,862
  parent-link indexes: **1,011,977 records in one active `.bace` base**.
- Complete derived-index validation passed against that actual pack, including
  membership and completeness. It took 85.31 seconds in a debug tooling run; this
  is an offline acceptance scan, not startup/tick work or a performance claim.
  Reindexing compared every logical source record with the previously accepted
  base before committing a replacement manifest.
- Actual Holtburg region preparation loaded 25 authored instances and one
  encounter from regional indexes without decoding the rest of the world.
  This is content preparation, not an authentic geometry admission or live NPC run.
- Independent compiled ACE/GDLE oracles cover new combat/lifecycle/player-description
  bytes, initial enqueue order/queues, character factory helpers, skill/reward/vendor/
  create-list formulas, door solidity, DAT geometry/motion/animation and scalar BSP
  placement/contact. Network inventory verifies 618 immutable official ACE Git blobs.
- Actual fingerprinted DAT checks decoded 256 sampled EnvCells, all 436 motion
  tables, 72 sampled animations, all 5,935 Setup records and all 772 Environment
  models. All 3,168 environment cells prepared into the GDLE query representation;
  9,504 bounded placement/contact query pairs completed. No proprietary bytes were
  added to fixtures. Full movement response and portal traversal remain unqualified.
- Default PostgreSQL tests use private disposable clusters. New scenarios include
  exact-request creation receipts, fenced nested inventory load, online/offline/item/
  housing saves, atomic transfer/CAS rollback, embedded identity rejection, restart
  fencing and stable IDs. Continuous critical inventory traffic cannot starve due
  routine owned-aggregate saves.
- Connected synthetic tests cover NPC pursuit, animation-hook attacks, death,
  source-selected drops, reserved XP proposals, receipt-gated corpse publication,
  respawn/decay and GDLE-led dynamic door use/occupancy. Reviews added regressions
  for blocked corpse decay, cross-cell home return, reserved IDs and timer limits.

The independent reward oracle caught a real precision mismatch: .NET's float Sum
accumulates in double before converting to float. The implementation was corrected;
fixtures and tolerances were not weakened. Review also found and corrected incomplete
reindex validation and embedded player/house identity mismatches before final checks.

Local development PostgreSQL is running in ignored `.local/postgres`, with no TCP
listener. `.local/server.env` and `.local/server.toml` identify it and the supplied
DATs/pack directory. `content-status` confirms content journal revision 0, **zero SQL
candidate heads**, pack generation 2 and one active `.bace` file. World source rows
were not bulk inserted into PostgreSQL; only accepted manifest metadata is stored.

`serve --config .local/server.toml` successfully inspected the accepted pack and DAT
presence, then returned its expected readiness failure without opening a game socket.
**No stock-client world entry or complete playable shard is claimed.** Remaining work
includes the main service loop, authentic movement/collision response/admission,
full object/private-property projection, native creation supplements, complete NPC
services/magic/treasure, production save/drain/hot-publication composition, paced
mixed-load qualification and Windows/macOS execution.

## Gameplay expansion validation — 2026-10-07

Validation of the shared working-tree snapshot used `cargo +stable` (1.96.1):

- `cargo fmt --all -- --check` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed: **631 tests**, zero failures, **22 opt-in
  tests ignored by default**. This run includes concurrent session additions;
  it is not a claim that every change was authored by this subagent team.
- `cargo xtask check` passed: **51 crates, 667 Rust files**, largest 832 lines.
- Seven opt-in supplied-DAT tests passed. Complete accepted-world index validation
  passed separately against the generation-2 manifest (1,011,977 records).
- All 21 PostgreSQL adapter tests passed using disposable private clusters. The
  runtime's five publication/persistence tests also passed, including native
  profile rejection, direct-SQL journal membership, bounded compaction/reopen,
  stale-completion/idle-head rejection and atomic rare/loot/XP rollback/replay.
- Two connected native-PvE regressions passed: frozen mutations/content hashes
  survive retry; rare state advances only with the exact receipt; a second player's
  final blow does not steal the top-damager's rare roll or change its own cursor.
- Independent source evidence includes 27 GDLE cast-control transitions, ACE magic
  serializers/mana/component calculations, DAT spell/component/formula fixtures,
  seven placement vectors, 42 rent vectors and 5,616 native-emote selection vectors.
  The emote oracle exposed a dotless-i comparison bug; the implementation was fixed
  without weakening the expected vectors.
- Independent Python HMAC verification and loot-domain tests passed, including
  interleaved-character isolation, nested selection, mutation bounds, reserved
  rare-item provenance and unchanged output on rejected expansion. The CLI's
  10,000-event synthetic sample completed without accessing any player stream.

Review added job correlation to the pack worker, checked the accepted head before
an idle publication result, retained bounded preparation capacity across cancelled
awaiters, and required an exact matching rare item in the death transaction.
Malformed missing/duplicate/mismatched/mutated/unearned rare proposals now reject.
Housing tests require joint house/payment preflight and reject standalone paid
house confirmation. These receipts are still trusted adapter inputs, not network
requests and not permission to acknowledge an uncertain database commit.

Local migrations through 0011 were applied. `content-status` still reported
journal revision 0, zero SQL candidate heads and **one active disk `.bace` file**
with 1,011,977 records. No synthetic loot or guessed rare policy was published into
that world. The initial opt-in world check used a crate-relative asset path and
failed to find it; the explicit absolute accepted-manifest path then passed.

Full stock-client playability, all magic/NPC service effects, authentic movement
admission/response, main gameplay/save/drain composition, platform qualification
and capacity/performance remain unverified or unimplemented as detailed in
`implementation-status.md`. Passing these checks does not remove the `serve` gate.

### Concurrent-edit qualification

The completed runs above are recorded evidence, not an assertion that an actively
edited shared tree remains unchanged. After this team's handoff, another session
continued adding runtime/session integrations. A subsequent focused test attempt
was blocked by unresolved `skill_device_output`, `decode_skill_device` and
`DispatchedSkillDevice` exports while those files were being written. Earlier
transient attempts similarly encountered an in-progress `dirty.csv` oracle file.
The subagent team's focused suites are complete, but final verification of all
concurrent additions requires a stable shared tree. Those edits were preserved;
no fixture was fabricated, test ignored, or conflicting work reset to obtain a pass.

## Integrated character, crafting and persistence validation — 2026-10-07

The integrating run completed after the transient export/fixture staging noted
above. All four required gates passed using `cargo +stable`: formatting,
workspace/all-target clippy with warnings denied, workspace tests, and
`cargo xtask check`. The final test run passed **652 tests**, with zero failures
and **22 opt-in tests ignored by default**. It includes all **24 PostgreSQL
adapter tests** against disposable private clusters. Architecture checking reports
**51 crates, 673 Rust files**, with the largest at 832 physical lines.

The affected supplied-DAT checks were also explicitly run: character creation/XP/
skill preparation (13 heritages, 38 skills) and bounded taboo-table decoding
(32 categories; 266 first-category patterns). Both passed against the approved
local portal DAT; no DAT content was added to the repository.

Crafting evidence includes 770 compiled official ACE chance vectors, 135 compiled
GDLE salvage vectors, and 1.28 million deterministic draws across 32 independent
character streams and four probability scenarios. The predeclared statistical
bound applies to each stream/scenario; the 33%/38% caps are owner policy, not a
claim of retail RNG equivalence. Skill/specialization evidence includes the
117-row transition and 302-row numeric fixtures. Enchantment restore adds 16
compiled ACE equipped-spell audit cases, alongside wire and Dirty Fighting
source fixtures.

Final integration regressions cover authenticated confirmation types 2/5/6,
queue pressure, exact durable receipts, reservation release rejection, current
versus base skill modifiers and expiry, player supplement composition, missing UI
owner rejection, and lazy equipment-set preparation. An unrelated carried set
item no longer blocks login; an applicable permanent effect still requires its
set assets. `git diff --check` also passed.

These results validate the implemented subsystem/adapter paths. Unsupported
recipe scripts/result-creation families, complete healer/missile action owners,
full stock-client orchestration and platform/performance qualification remain
explicitly documented in the subsystem READMEs and `implementation-status.md`.

## ACE generator integration validation — 2026-10-07

All four required gates passed using `cargo +stable`: workspace formatting,
workspace/all-target clippy with warnings denied, workspace tests, and
`cargo xtask check`. The final workspace run passed **891 tests**, with zero
failures and **31 opt-in tests ignored by default**. Architecture checking reports
**51 crates, 1,004 Rust files**, with the largest at 970 physical lines.
The run includes **27 PostgreSQL adapter tests** using disposable private clusters.
`git diff --check` passed. Earlier integration failures were fixed and the whole
workspace was rerun; no failing test was disabled or ignored to obtain this result.

The supplied-DAT run passed all seven generator admission and regional geometry
checks: native creatures, complete region encounters/building filtering, terrain,
Holtburg indoor/outdoor geometry, and human attack/cast motion chains. It used the
same fingerprint-verified portal/cell archives recorded above; no proprietary
assets were added to the repository.

Independent evidence includes 10,703 original C# table/enum rows, 120 creature
equipment cases, source death-drop filters, 40 equipped-spell routing cases and
28 NPC missile range/speed vectors. The spawning and loot oracle directories
retain the pin, extraction recipe and source hashes for selection, status,
placement, mutations, spells and special item families.

Integration regressions cover exact multi-item admission, missing-asset refusal,
nested lifecycle progress under saturated queues, retained vendor contributions,
permanent item-enchantment delay/reservations, and NPC projectile impact without
ammunition consumption. Death saves preserve complete item properties and nested
container lanes while overlaying accepted inventory quantities and ownership.
PostgreSQL regressions exercise first acquisition and durable world retirement,
including stale-revision atomic rollback and exact replay after epoch restart.
Recovery classification holds complete generator-linked world trees rather than
restoring stale remnants alongside fresh populations. Uncertain save replies
retain the immutable operation and reservations until exact resolution.

These checks qualify the implemented owners and adapters. Production bootstrap
must connect recovery retirement before generator initialization; full stock-client
service composition, all combat aura consumers, periodic/proc equipped effects,
platform qualification and representative capacity/performance remain gated.
The reviewed stash was preserved; its generator changes were already represented
or superseded in the current tree.
