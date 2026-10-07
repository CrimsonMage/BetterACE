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
