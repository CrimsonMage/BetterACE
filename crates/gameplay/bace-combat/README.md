# bace-combat

Attacks, damage, defenses and PvP rules.

Status: foundation; complete subsystem behavior is not implemented.

Implementation belongs in named modules. Crate roots remain declaration-only.

Scalar skill contests, explicit-time melee hook scheduling, and proportional kill-XP proposals are now implemented as bounded foundations. `skill_chance` preserves ACE's f32 multiplication before the f64 exponential. `kill_rewards` preserves ineligible damage in the denominator and ties-to-even rounding; it consumes an authoritative damage-history projection, not client damage. No award here is a durable acknowledgement. Full damage/defense/critical/weapon and corpse behavior remains unsupported.

Official source: ACEmulator/ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b`, `Source/ACE.Server/WorldObjects/SkillCheck.cs`, `Creature_Death.cs`, `Monster_Melee.cs`, `Player_Combat.cs`. AGPL-3.0-only; attribution remains with ACE contributors. GDLE `353cbab52ef7da2b7063bc3e3f008461d8531693` attack/death code corroborates sequencing only; its XP-by-level fallback is not adopted.

`death_messages` preserves the 38 source `Strings.cs` killer/victim/broadcast
templates and ACE DamageType selection, with a source-text golden for each damage
family and substitution. The caller must supply the accepted death blow, player
classification and a bounded random choice; templates alone do not deliver death
announcements or qualify every combat origin.

`tests/fixtures/skill.csv` contains 49 independent C# results. Regenerate skill, loot and vendor fixtures using `python3 crates/gameplay/bace-combat/oracle/generate.py --source .reference/ACE-47edade3bd3f6044b676d4eb877c4965c7eda62b --dotnet /path/to/dotnet`. The generator compares immutable upstream source bytes, records hashes, extracts verbatim methods and compiles them with synthetic adapters. Scheduling/reward boundary tests do not claim full ACE gameplay parity. Geometry rechecks intentionally harden the cases in divergence #7/#9/#10; synthetic integration is not stock-client qualification.

`oracle/rewards.py` additionally compiles verbatim `Creature.OnDeath_GrantXP`
against synthetic participant/history adapters. Its 72 vectors include unequal
contributions, ineligible sources, empty damage, midpoint rounding and large/small
mixed magnitudes. The oracle exposed .NET `Enumerable.Sum(float)`'s double
accumulation followed by one float cast; the Rust reference now preserves that
precision boundary. Fixtures establish raw death award proposals, not the later
complete `EarnXP` pipeline.

`specialization` implements scalar defense-rating bonuses, physical shield caps
and armor curves, magic shield absorption, Healing checks, Sneak Attack,
Recklessness and the trained/specialized Dirty Fighting spell selection. Inputs
are authoritative skill/equipment/geometry projections; randomized checks consume
explicit caller-supplied draws. Nonfinite and invalid ranges return errors.
`oracle/specialization.py` verifies official source bytes and compiles the verbatim
ACE methods with synthetic adapters using .NET 8. Its 302 frozen rows cover exact
f32 bits, thresholds and rounding. Added source paths are Creature_Rating.cs,
Creature_Combat.cs, SpellProjectile.cs, Healer.cs and SkillFormula.cs plus official
SpellId.cs; SHA-256 provenance is embedded in the fixture. Run it with the same
`--source` and `--dotnet` arguments as the other oracles.

Simulation consumes these bonuses in its prepared noncritical melee path and
magic projectile/registry paths. Accepted Healing and missile-defense projection
interfaces are available; a healer kit/vital transaction and complete missile
weapon attack pipeline remain unsupported. Helpers do not imply those gameplay
systems exist or establish full physical damage/critical parity.

## Prepared physical combat

The new `physical` path uses GDLE `353cbab52ef7da2b7063bc3e3f008461d8531693`
for melee/missile selection and scalar behavior, with ACE as corroboration.
`preparation` accepts immutable native weenies, exact equipment identities and
revisions, prepared motion hooks and owner-projected skills/enchantments.
It derives equipment roles and preserves body-part/quality components instead
of treating a template's scalar damage as a complete weapon calculation.

`oracle/physical_generate.py` compiles unchanged `CMeleeAttackEvent::Setup`,
`CDualWieldAttackEvent::Setup`, `GetImbueMultiplier`, `GetRatingMod` and
`GetSkillChance` against synthetic quality/CMT adapters. It verifies pinned git
objects and records source hashes. These vectors qualify selection and those
scalars; they do not yet prove the entire impact pipeline or stock movement.
The prepared simulation path has connected hook, cleave, PK-denial and
ammunition-receipt/projectile-impact tests. Wire missile decoding independently
compiles the unchanged official ACE action handler.

One explicit defect correction uses `100 / (100 - rating)` for negative ratings:
the pinned GDLE helper instead amplifies negative ratings and is singular at
-100. Positive/additive GDLE ratings are retained. Golden output records the
upstream result and tests the selected monotone correction; it is not labelled
independently proven retail behavior. Full damage, equipment refresh, source
approach/repeat and client projection must be qualified before claiming complete
retail combat.

Generated NPC missile range and animation speed follow the pinned ACE
`Creature_Missile.GetMaxMissileRange` and `Creature_Combat.GetAnimSpeed` methods.
`oracle/npc_missile.py` compiles their unchanged bodies with synthetic quality
adapters; `tests/npc_missile.rs` checks 28 recorded vectors and exact float bits.
The simulation's physical-combat regression exercises automatic NPC projectile
launch and impact without decrementing the equipped ammunition, matching the
disabled decrement in ACE's `Creature_Missile.UpdateAmmoAfterLaunch`.

Prepared physical admission rejects nonfinite power/accuracy and values outside
[0,1], including the adjacent float above 1 and the negative subnormal nearest
zero. Impact resolution separately validates original power before any arithmetic.
Raw wire decoding preserves the ACE float bits; authority rejection occurs on the
single simulation owner. Connected raw-packet and prepared-world tests distinguish
successful decoding from accepted damage.

Prepared animation deadlines now survive CancelAttack, mode changes and early
missile impact. A fresh sequence cannot restart an early hook before the original
prepared duration expires. Unsubmitted missile work can cancel on a mode change;
submitted ammo work retains its exact receipt and rejects cancellation or mode
changes until resolved. Retries do not create a second launch. Deadline records
are bounded by admitted physical actors, reclaimed on expiry, and exposed to
lifecycle transfer guards. These are anti-bypass protections around the current
prepared timing path, not qualification of the full retail charge/repeat driver
or authoritative DAT MotionPlayback impact integration. Pinned GDLE
Monster.cpp TryMeleeAttack/TryMissileAttack and AttackManager.cpp retain active
attack ownership; source StopCompletely retains active animation links.

The indexed local client review (`ClientCombatSystem::GetPowerBarLevel`,
`0x0056ADE0`) computes dual-wield power as elapsed / 0.8, whereas GDLE's manual
admission checks elapsed * 0.8 + 0.001. GDLE's own repeat scheduler uses power *
0.8 and supports the client's faster scale. The owner selects demonstrated client
behavior where these differ. The client also queues requests while waiting for
AttackDone and can build during an active attack, so a scalar replacement alone
would not implement the driver. The connected driver now retains one queued
replacement, overlaps manual charge with the current attack, uses the faster
dual-wield repeat interval and alternates on successful motion completion.
A complete differential client call-chain harness remains pending. The inspected client
serialization/collision helpers do not establish offhand damage or skill rules.
`docs/baselines.toml` records the source hash, index route and unconfirmed build
provenance; this source review is not a claim of tested client parity.

## Trusted physical sequencing and live projections

DAT-equipped actors use the World's physical motion domain for attack hooks and
completion. Profile-only timing remains a synthetic-body path; authentic geometry
bodies reject attacks without registered chains. Queued requests retain animation
and charge timing. Accepted server movement/turn controllers perform sticky
approach through swept physics, with bounded chase time and independent client
sequences. Missile release waits for Aim completion and durable ammunition
acknowledgement. Reload executes a trusted chain before another aim. Physical and
casting style changes execute source DAT links in both directions.

`oracle/physical_timing_generate.py --source /path/to/pinned/gdle --client
/path/to/ClientCombatSystem.cpp` compiles the unchanged local GetPowerBarLevel and
GDLE AttackManager::OnAttackDone bodies against synthetic adapters. Its 32 rows
record source hashes and the client's unconfirmed build provenance. Proprietary
source is never embedded. The vectors qualify numeric charge and repeat queue
replacement; connected tests separately cover floods, charge overlap, hand
alternation, cancellation, trusted hooks/reload and cross-mode sequencing.

Fast Arrows and LeadMissileTargets are sampled from accepted options at launch;
receipt retries retain that exact launch snapshot. Lifetime state separates target
hits, player resting ammunition, Destroy effects and final removal. Raw physical
refresh sources retain accepted actor/equipment content and exact item revisions.
Registry refresh reconstructs weapons, ratings, armor and resistance instead of
enchanting an already derived scalar. Skills and attributes update active melee
damage inputs without restarting its motion. Positive speed changes re-prepare
explicitly tagged DAT rates, retaining shared frame/hook arrays and Ready rates.
These paths still require stock-client qualification; prepared assets and focused
tests do not establish a fully playable shard.

Physical contact preparation now freezes source evasion, critical choice and weapon
base before weapon/Aetheria/Dirty effects. Post-proc resolution reads current
mitigation without rerolling the contact. The simulation retains keyed phases and
waits for spell descendants; target cloak reduction/healing precedes the final HP
write. GDLE `MeleeAttackEventData.cpp` 424–511, `Ammunition.cpp` 132–279 and
`WeenieObject.cpp` 4745 establish this ordering. Existing independent scalar
vectors still qualify the formulas; the contact/receipt tests qualify owner
sequencing and do not establish stock-client playability.
