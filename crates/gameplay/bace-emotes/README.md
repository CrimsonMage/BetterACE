# bace-emotes

Content-defined actions and NPC scripting.

Status: foundation; complete subsystem behavior is not implemented.

Implementation belongs in named modules. Crate roots remain declaration-only.

`EmoteScript` provides bounded explicit-time typed NPC sequencing: speech, delays, externally evaluated authoritative predicates, item/reward/quest effects, termination and cancellation. Effects block further work until the owning adapter acknowledges them; durable mutations require confirmed durable completion. Branch execution has a finite instruction budget. Construction rejects malformed branches and oversized strings/programs. The separate native interpreter below now owns category selection and numeric dispatch; this older EmoteScript interface remains a limited foundation. This older interface does not define native hand-in semantics; native hand-ins follow the source-defined stages described below.

Reference: official ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b`, `Source/ACE.Server/WorldObjects/Managers/EmoteManager.cs`; GDLE at the pinned secondary revision corroborates queued cancellation. This is a bounded orchestration foundation, not a claim of complete NPC script compatibility.

## Native interpreter

`NativeProgram` and `NativeEmoteManager` now consume the native typed `Emote`
collections directly. Category selection preserves the pinned category/quest,
vendor/WCID, heartbeat stance/motion and wounded-health filters, strict
`probability > draw`, lowest-probability selection and stable authored ties.
The compiled-source `oracle/native_generate.py` checks immutable ACE bytes and
runs the unchanged `GetEmoteSet` method; 5,616 CSV vectors cover these filters and
Unicode cases. They caught and fixed the difference between Rust uppercasing and
.NET OrdinalIgnoreCase for dotless i. Randomness is an explicit event-domain
input, not a process/thread RNG.

The interpreter dispatches all numeric actions 0–121 and 9001 into typed owner
operations, including native query/branch categories. Only pinned source no-ops
(0, 91–98, 100 and 111) report `SourceNoop`. Unknown opcodes reject preparation.
Delays, nested chains, instruction count, pending completion tickets and strings
are bounded. Owner `Pending` never means completed gameplay: the VM does not
advance that action until its matching completion. Notification capacity pressure
retains the next action while physics continues.

The simulation adapter currently connects authoritative property/quest queries,
property mutations, quest updates, XP spends and queued recipient grants, luminance grants/spends,
world event state and fellowship quest/lock metadata to their owning domains.
Mutative proposals retain exact before/after revisions until a durable receipt
is supplied. Connected tests inspect real owner state before and after receipts,
branch-dependent speech and held logout/progression. Native numeric dispatch is
broader than completed owner integration: inventory grants/takes, casting,
movement, generated treasure, contracts, UI and other delegated actions remain
explicit service requests until their owner has actually completed them. They
cannot be acknowledged through the generic local mutation receipt path.

Hand-ins follow source-defined stages: accepted input consumption precedes the
Give set, each row has its own effect boundary, and detached Give/cast/motion
chains do not make the outer script wait for an invented whole-hand-in commit.
The entry adapter must establish accepted input consumption before invoking the
Give category. Independent durable owner services remain explicit proposals;
the interpreter never grants success merely because it emitted one.

The unchanged pinned ACE `ExecuteEmoteSet`, `Enqueue` and `DoEnqueue` methods
are compiled by `oracle/timeline_generate.py`. Differential cases cover row
pre-delays, prior actual-execution post-delays, delayed execution, nested timer
ordering and terminal busy-state release. Immediate row continuations drain on
the simulation owner with an explicit per-source budget; nested immediate calls
have priority over their parent's continuation. Checkpoints retain this call
priority, stable timer order, pending versus detached tickets, and bounded work.
The source owner retains separate event/key/cursor descriptors for overlapping
invocations, so restarting or starting another script cannot reseed an older
pending service. Snapshot conversion uses separate frozen DTOs and exact
participant revisions.

Title/sanctuary/contract and recipient XP/level/credit mutations now use their
character/quest owners. Property and quest proposals separately fence local
registry revisions and player aggregate revisions. Live query views read
inventory, current skills/vitals/attributes and canonical character metadata at
execution time. Full service coverage still requires the named service adapters
and packet projections; numeric dispatch, timeline tests and prepared proposals
alone are not a claim of complete native-NPC or stock-client parity.

The native NoShareExperience path now preserves `Player_Xp.GrantXP` recipient
queue ordering. The source opcode captures the modified amount, requests durable
workflow-only admission, then detaches and drains immediate outer/nested rows.
Only afterward does the queued recipient prepare XP, level and skill-credit
changes from current character state. The admission and recipient mutation have
distinct receipt types; recovery retains the queue phase and requires participant
owners to be loaded before either queries or recipient reconciliation resume.
No whole-hand-in transaction or fabricated XP acknowledgment is introduced.

`oracle/queued_xp_generate.py` byte-verifies and compiles unchanged pinned ACE
`EarnXP`, `GrantXP`, `UpdateXpAndLevel`, `CheckForLevelup`, `GrantItemXP`, and
native enqueue methods. Its two connected traces cover a zero-delay old-level
query and an intervening TotalXP mutation; three numeric cases preserve C#'s
float product before double-rate multiplication and midpoint-to-even rounding.
The harness substitutes an explicit owner queue and empty equipped roster; it
throws on unsupported sharing, vitae and item branches. Simulation tests compare
actual owner state to these fixtures, including admission and recipient recovery.

The minimal unconfigured XP fallback still restricts equipped items, vitae and
non-full level-up vitals. Configuring the shared reward owner connects the full
existing player/item/allegiance/vitae proposals instead. Level-proportional XP
uses allegiance-only sharing; ordinary positive XP uses all sharing, while
NoShareExperience uses neither fellowship nor allegiance. Frozen NPC workflow
schema 2 preserves this distinction across restart and explicitly migrates old
unshared/shared schema 1 variants. The unchanged C# `GrantLevelProportionalXp`
and `AddSkillCredits` methods produce the independent `level_xp.csv` and
`training_credits.csv` fixtures; the latter preserves nullable signed counters.

Named simulation service adapters now connect Give/Take inventory proposals,
canonical TeachSpell, signed training credits, source ResetSkill, casts, prepared
world motion, swept server movement/turning, explicit door requests, generator
selection, local signals and target teleports to their owners. Valuable effects
join an adopted source checkpoint in the same journal transaction. An initial
hand-in has a separate transaction for input consumption and the unexecuted Give
continuation; no reward row runs before that exact receipt. Detached service
admission preserves authored post-delays while retaining the unfinished effect.
Recovery requires participant owners before any row resumes. Runtime has explicit
freeze adapters and a bounded typed simulation command/result lane; these do not
by themselves establish a complete stock-client service loop.

LocalSignal uses PropertyInt 290/291, same-landblock listeners, source registration
order and accepted collision cylinders. Its distance arithmetic follows the
unchanged pinned `Physics/Common/Position.CylinderDistance`, compiled by
`oracle/signal_generate.py`; this includes the source's float intermediates and
signed overlap branch. Signals prepare all listener VMs before mutating them and
do not insert a line-of-sight requirement absent from the source.

Remaining delegated families still require their named owner integrations; a
numeric opcode or a generic callback is not evidence of gameplay completion.
Full stock-client movement/animation, packet output, lifecycle recovery and
production NPC qualification remain separate acceptance work.

NPC use-radius preparation also preserves signed and zero Float54 values. Pinned
`WorldObjects/WorldObject_Use.cs` defaults only an absent value to0.6 and compares
against the float-cast signed cylinder distance above. The registration boundary
rejects nonfinite values; it does not reject an entire region for a negative
threshold. Use/hand-in retains the existing accepted same-cell/line-of-sight
safety gate in addition to this distance test; full cross-cell use parity remains
unqualified.
