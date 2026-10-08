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
or a durable completion receipt. Native quest definitions and registry mutations now have the separate implementations described below. Complete catalog/session loading, contracts, atomic reward transactions and notification composition remain unsupported.

Implementation belongs in named modules. Crate roots remain declaration-only.

`QuestRegistry` now provides bounded, revision-fenced proposals/adoption for
Update/Stamp, Erase, Increment/Decrement, completion counts and quest bits. It
strips `@` comments and uses canonical case-insensitive lookup keys, preserves
signed counters, explicit source timestamps, first-solve behavior and maximum
solve rules. Overflow and stale receipts reject before mutation. Simulation
connects these operations to native NPC query/branch execution and actual
receipt-gated quest state; the existing independent eligibility oracle remains
separate from this new registry integration evidence. Complete contracts and
atomic multi-action hand-in coordination are still unsupported.


`ContractRegistry` now owns bounded contract membership and display selection
(maximum 100), exact before/after proposals and adoption. Duplicate adds preserve
membership; the source full-registry check precedes duplicate acceptance. Native
NPC integration validates definitions against the accepted DAT catalog, emits
the source full-registry message, and evaluates `InqContractsFull` from the actual
owner. Contract status/quest tracking and client projection still require the
prepared DAT definitions and current quest owner; membership alone is not that
complete qualification.
