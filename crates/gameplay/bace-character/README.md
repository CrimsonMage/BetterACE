# bace-character

`propose_attribute_transfer` follows pinned ACE
`AttributeTransferDevice.ActOnUse/VerifyRequirements`: any equipped Attrib or
RawAttrib requirement blocks; the source must exceed 10 and the destination
remain below 100. It proposes the minimum of 10, source surplus and destination
room without mutating the live aggregate. Exact adoption changes both innate
starting values under one revision, preserving XP and ranks. Same-attribute
authored content follows ACE's net-zero mutation. Two source-derived boundary
tests cover the proposal and stale adoption. Simulation and runtime own item
consumption and confirmation.

Character creation, progression, attributes and vitals.

Status: foundation; full character lifecycle is not implemented.

`CreationRules` validates the six starting attributes and exactly 55 skill
allocations against explicitly supplied heritage budgets and resolved DAT skill
costs. It returns a proposed allocation, not a created character. It preserves
pinned ACE's trained-skill 526 XP/rank 5 bonus and specialized-skill initial level
10, permits unused credits, rejects unknown active skills and cannot spend
negative credits. No default DAT table or heritage budget is fabricated.

`CharacterProgression` implements bounded, atomic attribute/vital/trained-skill/
specialized-skill XP expenditure against caller-supplied immutable cumulative XP
tables. `RankTable` validates tables before allocation and uses a bounded binary
search, including ACE's highest-rank behavior for repeated thresholds. Tables are
shared through `Arc`; expenditure does not allocate. Successful mutations advance
an aggregate revision; zero-XP requests retain the revision. Rejections retain all
state. This is in-memory progression, not a durable-operation acknowledgment.

`with_state` accepts complete authoritative `TraitState` values: starting values
for attributes/vitals, current vitals, and skill initial level/resistance/last-used
time. It checks target/detail consistency and finite skill times. Frozen
before/after projections include these values and the advancement class. The old
`new` constructor explicitly has no details (`None`); replication must reject
incomplete projections instead of filling missing values with zero.

`with_training` attaches shared prepared base DAT costs, authoritative available
skill credits and an explicit set of augmentation-specialized skills. Invalid
configuration returns the original aggregate. `train_skill` checks the client's
signed quoted cost against the exact trusted price, spends credits, resets normal
trained skill XP/rank/initial level to zero, and advances the same dirty revision.
It does not apply the creation-only 526 XP bonus. `specialize_skill` is a
server-only transition preserving XP, calculating specialized rank and setting
initial level 10. Device/quest eligibility and durable item transactions remain
the caller's responsibility. Usage metadata survives both transitions.

The five tinkering/salvaging augmentations specialize trained skills at zero skill-credit cost, preserve XP and restore specialization on retraining. Missing skill records may be created only for known
prepared skill definitions, with the explicit fresh defaults in ACE
`Creature.GetCreatureSkill`/`PropertiesSkill`; missing details on existing records
are rejected. All failure paths preserve state, credits and revisions. Maps remain
bounded to 256 traits/definitions. Preparation and occasional new-skill insertion
allocate; ordinary expenditure and existing-skill transitions do not.

Authoritative provenance is official ACEmulator/ACE, commit
`47edade3bd3f6044b676d4eb877c4965c7eda62b`, by the ACE contributors:

- `Source/ACE.Server/WorldObjects/Player_Attributes.cs`: `SpendAttributeXp`, `CalcAttributeRank`.
- `Source/ACE.Server/WorldObjects/Player_Vitals.cs`: `SpendVitalXp`, `CalcVitalRank`.
- `Source/ACE.Server/WorldObjects/Player_Skills.cs`: `SpendSkillXp`, `CalcSkillRank`, `GetSkillXPTable`.
- `Source/ACE.Server/WorldObjects/Player_Xp.cs`: `SpendXP`.
- `Source/ACE.Server/Factories/PlayerFactory.cs`: `ValidateAttributeCredits` and the creation skill-allocation loop.
- `Source/ACE.Server/WorldObjects/Player_Skills.cs`: `TrainSkill` and `SpecializeSkill` creation branches.
- `Source/ACE.Server/WorldObjects/Player_Skills.cs`: `HandleActionTrainSkill`, `TrainSkill` without creation bonus, `SpecializeSkill(resetSkill=false)`.
- `Source/ACE.Server/WorldObjects/Creature_Skills.cs` and `Source/ACE.Entity/Models/PropertiesSkill.cs`: explicit newly instantiated skill defaults and usage fields.
- `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateAttribute.cs`, `GameMessagePrivateUpdateVital.cs`, `GameMessagePrivateUpdateSkill.cs`: authoritative projection fields; packet encoding remains in network crates.
- `Source/ACE.Entity/Enum/Properties/PropertyAttribute.cs` and `PropertyAttribute2nd.cs`: numeric property identities.

`tests/fixtures/progression.csv` records 1,408 results generated by compiling
verbatim official method bodies against synthetic XP tables and harness-only
player, trait, network and logging adapters. Its source hashes are checked against
immutable GitHub bytes during generation. The harness does not run full ACE
handlers or serialize messages and does not prove DAT, formula, notification,
full character creation or stock-client compatibility. `creation.csv` adds 320
official attribute-validation and fresh skill-initialization scenarios. It
invokes verbatim methods with augmentation effects disabled; template-derived
initial skills, augmentation specialization and full factory execution are not
covered. No proprietary DAT is included.
`training.csv` adds 270 official handler/training/in-world-specialization cases,
including success/failure, signed quoted prices, all advancement classes and XP
preservation. Harness-only network adapters discard response packets; response
byte compatibility belongs to the independent wire/replication fixtures.

Regenerate with Python 3, network access and .NET 10:

```sh
python3 crates/gameplay/bace-character/oracle/generate.py \
  --source .reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b \
  --dotnet /path/to/dotnet
cargo test -p bace-character
```

Hardening beyond upstream: malformed/oversized/decreasing tables, impossible
loaded XP, untrained skill XP, duplicate trait identities and revision exhaustion
fail before mutation. IDs do not truncate a client u32 into a property ushort.
Regression tests cover both official valid inputs and these invalid boundaries.
Ranks are derived from XP; manually edited rank/XP inconsistencies are not an
import contract. Maximum rank comes from the prepared table, not a hard-coded 190.
Creation failure returns no partially mutated actor even when ACE's intermediate
factory state was mutated. User divergence #36 is explicitly regression-tested:
known locked skills must be trained or specialized at creation, an explicit user-approved correction to pinned ACE. Lowering preserves their trained status while refunding invested XP (0x4D9). Name taboo/creature checks, durable
uniqueness, appearance and initial template validation must precede creation;
successful allocation validation is not a substitute for any of those checks.
User divergence #14 is owned at the network integration boundary: retail's public UpdateSkillAC has a client-backed codec. Domain projections preserve all skill fields so full updates can supplement advancement-only messages where PP or initial level changes. Attribute-transfer responses (#40) remain unsupported here.

The simulation owner must bind authenticated sessions to actors, order requests,
refresh derived stats, produce response effects and schedule saves. Full creation,
complete character levels/earned-XP distribution, vitals regeneration and durable playable world composition remain incomplete. Device ownership and consumption belong in the reserving simulation/persistence operation.
Track complete scope and reported ACE/retail differences in
`docs/gameplay-parity.toml` and `docs/divergences.toml`.

Implementation belongs in named modules. Crate roots remain declaration-only.

`prepare_character` now composes a bounded fresh humanoid character proposal from
neutral, immutable projections of verified CharGen/vital/palette/clothing assets,
accepted human/item templates and server-allocated IDs. It validates allocation,
appearance choices, hues, heritage/gender/template/start selection, name-policy
approval identity and content presence before returning player/possession
snapshots, appearance metadata and character options. Missing clothing or starter
items fail atomically. It grants no client-selected privilege. Runtime must still
perform actual taboo/creature checks, admit collision geometry and atomically
reserve name/account/IDs with the complete durable create. Olthoi-specific creation,
full character/runtime playability and customized-template edge cases are not
qualified by this factory foundation.

`oracle/factory_generate.py` compiles verbatim pinned `PaletteSet.GetPaletteID`,
`AttributeFormula.GetFormula`, `FloatExtensions.Round`, and PlayerFactory heritage
mastery/augmentation methods. `tests/fixtures/factory.csv` has 108 independent
vectors. AttributeFormula uses **AwayFromZero**, unlike the default ties-to-even
`Math.Round` used by kill rewards. The harness does not execute full PlayerFactory;
proposal integration tests exercise immutable input, missing assets/IDs and invalid
selection without claiming complete creation parity.

`ExperienceCredit` carries exact before/after spendable-XP and aggregate revision
values for a valuable-operation proposal. Simulation reserves participants until
the persistence adapter confirms these exact after-values were committed, then
adopts the same values. This does not implement complete EarnXP level changes,
XP multipliers, fellowship/allegiance distribution or prestige systems.

Humanoid preparation additionally applies the persistent defaults in pinned
`Source/ACE.Server/WorldObjects/Player.cs`'s fresh-weenie constructor: absent total
and available XP become zero, Attackable becomes true, and the humanoid create
list is cleared. Initialized XP values remain intact. The constructor regression
in `tests/factory.rs` checks these changes without mutating the source template.
DateOfBirth (`PropertyString` 43) is a **required lifecycle supplement** from
explicit server creation time before durable creation. The factory removes any
inherited template DOB and never reads a wall clock. The existing helper oracles
still do not establish complete Player constructor/PlayerFactory parity.

## Skill transitions and values

`lower_skill` implements temple one-step unspecialization/untraining with exact XP
and credit refunds, locked-skill XP-only recovery, augmentation XP-only recovery,
and four-slot prepared wield-requirement rejection. `reset_skill` implements the
separate NPC full-reset behavior: augmented tinkering can become untrained while
retaining its augmentation; augmented Salvaging stays specialized. All arithmetic
and revision checks precede mutation. The 70-credit specialization budget uses
full DAT prices with heritage PrimaryCost overrides, separately from the upgrade
price paid. None and augmentation skills do not count toward the existing total.

Specializing trained-max skills preserves legitimate XP above the specialized
curve maximum, matching the explicit retail note in pinned `CreatureSkill`.
Load validation admits this only through the trained maximum; rank caps, further
expenditure fails, and later refunds return the entire stored amount.

`SkillProposal` privately owns before/after snapshots for a valuable operation.
The proposed actor has no mutable access; only the single live owner can adopt it
after exact durable success and matching before-state/revision. Snapshot work is
bounded to 256 traits and occurs once per valuable operation, not per tick.
`touch_revision` lets the same owner mark UI/enchantment supplements dirty.

`skill_values` computes authoritative Base/Current from explicitly supplied DAT
attribute formulas, attribute projections, augmentation/luminance/enlightenment
bonuses, multiplicative enchantments, vitae and additive modifiers in pinned
operation order. Post-vitae bonuses are not scaled by vitae. Numeric assumptions
are scalar IEEE f32 and ACE's AwayFromZero rounding; nonfinite/overflow inputs
fail. Combat/healing/crafting consume immutable values through subsystem APIs.

`tests/fixtures/skills.csv` contains 117 independent rows generated by compiling
verbatim pinned Player_Skills lowering/reset/specialization/refund methods and
CreatureSkill bonus methods. Source bytes are compared to the immutable official
GitHub pin and SHA-256 hashes are recorded. The harness supplies synthetic state
and discarded network adapters; it does not prove full device or client behavior.
Regenerate with `oracle/skill_generate.py --source <pinned ACE> --dotnet <SDK>`.

## Name approval

`NamePolicy::prepare` consumes the first authored DAT taboo category and the
complete accepted creature-name index. `approve` creates a non-forgeable
`NameApproval` for the original input plus normalized saved name. The factory
checks the input binding and writes the normalized name consistently. Database
uniqueness remains part of the later atomic create.

The explicit project policy keeps the 100-byte server limit, Windows-1252
representability, an uppercase initial and letters/apostrophes/hyphens/single
internal spaces. `ALICE` becomes `Alice`, `alice` is rejected and `Alice smith`
is retained. This is not a claim to reproduce retail FormatName's 32-byte filter.
Taboo matching is case-insensitive and word-local with literal `*` wildcards;
unsupported regex patterns fail preparation instead of silently being reinterpreted.
Name/creature checks run on prepared immutable input, with no database access in
character logic. The opaque token does not assert a durable uniqueness reservation.

`LuminanceState`/`LuminanceCredit` now prepare and adopt revision-fenced luminance
changes with the pinned quest/enchantment multipliers, ties-to-even rounding,
maximum cap and insufficient-funds behavior. Negative grants/spends, invalid
modifiers and overflow fail before mutation. `ItemExperience` supports the three
valid item-XP schemes, level thresholds, capped XP proposals and exact adoption.
`aetheria_proc_rate` preserves the f32 level/augmentation/mode calculation and
explicit random comparison; self-targeted surge IDs are retained.

`oracle/extended_generate.py` verifies pinned official source bytes and compiles
unchanged `Player_Luminance.cs`, `ExperienceSystem.cs` and `CalcProcRate` against
small owner/configuration adapters. `tests/fixtures/extended.csv` contains 404
independent vectors. Invalid/over-cap item state and undefined XP style are
excluded from valid-state parity and fail explicitly; FixedPlusBase below its
base returns level zero instead of reproducing the source's unsigned underflow.
Connected native-NPC tests adopt actual XP/luminance only after matching receipts.
Full equipped-item XP persistence, Aetheria proc-to-spell effects and broader
EarnXP/level/reward composition remain separate integration work.

`adopt_combined_reward` permits one aggregate revision to cover both spendable XP
and a committed rare-state change. Zero-XP eligible rare attempts still require a
new mutation revision. Character lifecycle transfers must retain rare state;
progression-only removal refuses when additional authoritative state is present.


NPC character services now retain canonical titles, level/total XP, enlightenment
and sanctuary independently of generic world-property copies. Recipient XP
proposals update total/available XP, level and training credits with one aggregate
revision. `oracle/earned_generate.py` runs unchanged ACE `UpdateXpAndLevel` and
`CheckForLevelup`; the fixture covers caps and multi-level credit changes.
Sharing, item XP and allegiance/vitae effects remain explicit owning-service
stages rather than being implied by adding spendable XP.

`vitae_experience` returns the exact pool transition, actual registry float,
normalized return value and the source two-second removal request. Magic owns
registry adoption and timers. The unchanged ACE update/threshold/reduction
oracle covers 160 cases, including preservation of the raw float near 1.0.
Invalid values and overflowing pools fail before owner mutation.

`ProgressionSnapshot` provides a bounded immutable read for save preparation while
the live character remains on its simulation owner. It copies at most 256 trait
entries and shares immutable tables. It exposes no mutation or ownership-transfer
API and is not serialized directly. The revision-race regression proves a queued
snapshot keeps its original values while the live owner continues changing.

Recipient level rewards now retain nullable signed TotalSkillCredits (Int23) in
CharacterServiceState and its exact before/after proposal. Pinned ACE uses an
`int?` property: adding level credits to an absent value leaves it absent, while
present signed values advance. Available credits remain in their existing
progression owner; both values adopt under one aggregate revision. Arithmetic
is checked, and overflow or a mismatched total-credit receipt leaves all owner
state unchanged. Int23 NPC reads and writes use this canonical state.

The regenerated earned-XP oracle has 72 unchanged-source cases covering present,
absent and negative initial total credits across zero, single and multiple level
crossings. Tests compare actual adopted total credits with the oracle, rather
than reconstructing an expected total outside the owner. Connected NPC tests
verify both credit owners remain unchanged until the receipt and survive the
complete player-state transfer. Workflow DTOs freeze the nullable field explicitly.

Vital and attribute projections preserve ACE `CreatureVital.GetMaxValue`,
`CreatureAttribute.GetCurrent` and `AttributeFormula.GetFormula` operation order.
Health enlightenment/gear bonuses precede multiplication and vitae; ordinary
additives follow them. Source minima and float rounding are retained. The oracle
compiles those original methods and generates 366 independent golden cases;
nonfinite inputs and arithmetic overflow return errors before mutation.

`propose_proficiency` stages pinned `Entity/Proficiency.OnSuccessUse` without
mutating the live character: 900-second skill-use resistance/time gate, negative
clock repair, Olthoi/untrained/max-level guards, float32 arithmetic and ties-even
rounding, maximum-character/skill XP caps, and exact usage metadata. ACE queues
`GrantXP.UpdateXpAndLevel` but immediately calls `HandleActionRaiseSkill`; the
proposal therefore spends only pre-award available XP. An insufficient old XP
balance still receives the queued grant but does not spend proficiency points.
One aggregate revision joins usage, spent/granted XP, levels and credits with a
caller-owned valuable operation. The simulation separately stages level-up vitals
and Vitae and adopts everything only after that operation commits. No fellowship,
allegiance pass-up or item XP is implied by `XpType.Proficiency/ShareType.None`.

`tests/fixtures/proficiency.csv` has 1,512 vectors produced by compiling the
unmodified pinned `Entity/Proficiency.cs`, with adapters that retain the actual
queued-grant/immediate-spend order. It covers cooldown boundaries, backward time,
Olthoi and untrained actors, max-level/max-skill cases and insufficient pre-award
XP. The fixture is not a full action-queue or network capture. Regenerate with
`python3 oracle/proficiency/generate.py /path/to/dotnet` (SDK 8 or compatible).
Nonfinite inputs and revision exhaustion reject without partial adoption.
