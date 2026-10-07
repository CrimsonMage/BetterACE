# bace-quests

Quest state, counters and contracts.

Status: foundation; full quest/contract gameplay is not implemented.

`next_solve` evaluates explicitly supplied immutable quest definitions, progress,
Unix seconds and configured cooldown rate. It distinguishes absent content,
maximum solves, waiting and readiness; preserves first-solve ordering (including
maximum zero); truncates scaled positive intervals like ACE; and preserves the
case-sensitive `ColoArena` exemption. No database, clock, packet, allocation or
world mutation is hidden in evaluation. `has_solves`, `has_bits` and `has_no_bits`
preserve signed registry values and missing-record behavior.

Source provenance: official ACEmulator/ACE contributors, commit
`47edade3bd3f6044b676d4eb877c4965c7eda62b`,
`Source/ACE.Server/Managers/QuestManager.cs`: `GetNextSolveTime`,
`CanScaleQuestMinDelta`, `HasQuestSolves`, `HasQuestBits`, `HasNoQuestBits`.
`tests/fixtures/eligibility.csv` contains 4,610 independently generated cases.
The harness compiles verbatim official method bodies with synthetic database,
clock and configuration adapters; it does not reproduce formulas in expected
values. Generation verifies local source against immutable upstream bytes and
records its hash. No real player state or proprietary DAT is included.

```sh
python3 crates/gameplay/bace-quests/oracle/generate.py \
  --source .reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b \
  --dotnet /path/to/dotnet
cargo test -p bace-quests
```

Hardening: invalid/negative/nonfinite rates and overflowing scaled intervals or
deadlines return errors instead of wrapping into immediately solvable quests.
Boundary tests cover these intentional deviations alongside official valid cases.
The caller must supply authenticated authoritative progress and explicit time;
network timestamps are not a clock source. Readiness is not reward authorization
or a durable completion receipt. Quest definition import, registry mutations,
quest-name resolution, contracts, reward transactions, emotes and notification
composition remain unsupported.

Implementation belongs in named modules. Crate roots remain declaration-only.
