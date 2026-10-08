# Native loot and rare policies

BetterACE authoring uses TOML; runtime lookup uses frozen binary records in the
accepted aggregate `.bace` generation. PostgreSQL contains incremental candidate
journal entries and accepted manifest/head metadata, not a second copy of the
bulk imported world.

## Authoring and verification

`LootGraphV1` describes named nodes and branches. Selection is explicit: all,
weighted, independent Bernoulli, ordered cumulative, item, mutation, or nothing.
Weighted probabilities normalize their authored weights; ordered cumulative
probabilities retain their residual no-drop interval. Named branch identity and
repeat index separate nested RNG streams. Item children may choose further
material/stat/spell mutations; conflicting mutations of one property reject.
Graph cycles, depth, expansion, stack sizes and output are bounded.

Examples are in `tests/fixtures/loot/`. They are synthetic tests with placeholder
template IDs, not enabled retail treasure tables.

```sh
cargo +stable run -p bace-cli -- loot-sample --input tests/fixtures/loot/nested.toml --events 10000
cargo +stable run -p bace-cli -- loot-build --table tests/fixtures/loot/nested.toml --rare-profile tests/fixtures/loot/rares-unconfigured.toml --output-directory /tmp/betterace-loot-example
```

`loot-build` requires a new output directory and writes one aggregate supplement.
It does not change the accepted world. `loot-publish --table <file>` (optionally
repeated, and/or `--rare-profile <file>`) queues incremental binary candidates in
the durable journal. The application must run
`bace_runtime::native_publication::publish_native_once` to validate identities and
references against its accepted world and accept the prepared generation. This
API is tested; the main stock-client service is not yet composed.

The worker reserves delivery before acceptance, uses the dedicated pack-I/O
owner, and compacts before exceeding one base plus two active deltas. Save pressure
can defer compaction without losing pending work. Rejected transactions preserve
the accepted heads. Direct SQL native inserts use the same journal trigger.
Mixed weenie/profile transactions are unsupported by the native-only worker and
remain pending for explicit handling; they cannot be partially accepted by the
legacy weenie worker. Reader-held retired pack files remain immutable and require
separate safe reclamation.

## Rare evidence and isolation

`docs/rare-evidence.toml` preserves the supplied Turbine evidence and uncertainties.
An eligible death is a lootable monster whose level exceeds the credited killer's
level or is at least 100. Rare occurrence, tier and concrete item are separate
stages. The normal opportunity stays active while the separately scheduled
real-time opportunity waits. Both may succeed, but one death produces at most one
rare in the shared corpse. Killer attribution belongs to the combat owner and
uses credited damage, not a per-contributor or final-blow roll.

Only rares retain a per-character stream ordinal and real-time state. Ordinary
loot uses event/table-local streams. HMAC-SHA256 domain separation prevents other
players, normal loot, NPC activity and sibling table draws from advancing that
rare stream. This is an intentional BetterACE isolation policy, not a claim to
have recovered Turbine's PRNG. Independent Python vectors freeze algorithm-v1
framing; tests cover interleaved characters, no-drop paths and deterministic retry.

There are no default retail rates. In particular, 1/2500 remains a community
estimate, and historical ratios with an unrecovered original source are not
silently turned into active tier weights. Enabling a profile requires explicit
probabilities, all six tier distributions, concrete item weights, timer interval,
distribution, initialization/reset policy and evidence notes. The supplied draft
is disabled. Uniform-second sampling is an available operator policy, not an
assertion about retail's interval distribution.

`rng-init --key-file <private-path> --version 1` provisions a Unix-private key and
binds its fingerprint in PostgreSQL. Existing keys are not overwritten and an
existing version cannot be rebound to different material. Keys must accompany
protected database backups. Automatic private-file provisioning on Windows is
not qualified; externally supplied key loading remains separate from gameplay.

## Durable outcomes

Character RNG identity/counters and timers are frozen in player save schema 2;
later schema supplements preserve them. V1 migration does not invent a seed or
timer. The bootstrap identity is deterministic for the character under the
bound server key. Schema downgrade, clearing/reseeding existing rare state and
counter/time rewind are rejected by persistence identity checks.

A native death proposal retains event identity, graph/profile/content hashes,
full generated mutations and rare before/after state. The runtime death freezer
creates one placement operation containing player spendable-XP/rare changes,
corpse and generated items. A stable event-derived operation ID plus database
request fingerprint prevents rerolling a retry. CAS failure rolls the whole
operation back; replay of the exact committed request returns its receipt. The
simulation adopts only a matching durable confirmation. An uncertain commit must
be resolved while the participant state remains reserved.

This does not complete original retail treasure mutation tables, full ACE EarnXP
processing, stock-client loot UI, corpse permission projection or the playable
world service loop. Those boundaries remain tracked in implementation-status.md.

Publication cancellation is fenced by monotonically correlated pack-job IDs and
an accepted-head check even when the pending journal is empty. Reusing a worker
with an orphaned completion fails before head acceptance. The bounded preparation
permit travels with the blocking validation result, so cancelling an awaiting task
does not release capacity while its work remains alive. Native graph node ID
`$rare` is reserved: the death freezer requires exactly one unmutated stack-one
item matching an awarded rare, and rejects missing, duplicate, mismatched or
unearned rare items before constructing an operation.
