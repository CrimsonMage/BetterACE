# bace-magic

Casting, spells and enchantments.

Status: connected foundation. Authoritative owner integration and synthetic tests
are implemented; this is not complete five-school gameplay or stock-client
playability. Verification of real motion/geometry assets remains a separate gate.

The owner-selected reference order is verified retail evidence, then pinned GDLE
for combat/cast authority, with pinned ACE supplying remaining behavior and exact
wire layouts. Pins are GDLE `353cbab52ef7da2b7063bc3e3f008461d8531693` and ACE
`47edade3bd3f6044b676d4eb877c4965c7eda62b`. Preserve upstream attribution and
AGPL-3.0-only. Existing divergence observations 11–13 apply to casting; the original
private register is not copied into fixtures or documentation.

`CastDriver` implements the selected GDLE policy for NPK and PK: accepted server
heading gates release at 45 degrees; displacement >=6 m disrupts; gesture timeout
is four seconds; turning hold is finite at 9999 seconds with 1.25-second retries.
Only one controlled cast runs per actor. Independent server item/emote effects use
explicit `CastOrigin`, never forged player sessions. `Release` remains a request
until the owning effect/resource operation resolves it. Client pose, timestamps
and motion acknowledgements cannot authorize completion.

Production gestures now carry `Arc<PreparedMotionChain>`. The World owner advances
the GDLE double cursor, applies root motion through collision, and emits
actor/epoch/domain/cast/action-fenced completion and hook events. Gameplay's
minimum-cast interval remains distinct from clip duration. Missing motion chains
refuse admission. Duration-only timers require explicit synthetic fixture opt-in
and are prohibited for authentic bodies. Player chains must be proven rootless to
preserve legal server locomotion during casting; composition of player movement
with root-bearing cast programs remains gated. Runtime `prepare_cast_gestures`
selects GDLE base-formula gestures, skips invalid low-zero commands/fast scarabs,
and resolves each through prepared motion links. Per-actor gesture preparation
preserves account-specific component overrides. Substate gestures update the
preparation context; repeated substates retain the live World cursor instead of
restarting their animation. EndCast admits the source Ready suffix while retaining
active action links and tagged callbacks. A bounded `MotionStopped` event carries
the actual accepted style/substate/rate after stop admission; it is not an action
command. Terminal outcome pressure retains that projection and completed result,
never reruns an effect or changes its recovery timestamp.

The fingerprinted local-asset qualification in runtime `tests/magic_preparation.rs`
prepares 6,266 spell-table entries / 13,538 sequential gestures with no unresolved
chain and verifies rootlessness. This proves preparation coverage for that DAT,
not every effect or stock-client playback. Bounded World tests and original GDLE
C++ cursor/link/queued-stop oracles establish their separately stated execution
cases, including retained callbacks and continuation behind queued Ready links.

Player mode handling follows GDLE's accepted interpreter style. A noncombat
style observed by Update emits the original fizzle intensity, removes up to five
mana and ends use successfully without components, normal spell effects or a
successful-school timestamp. Melee/missile style is not a blanket cancellation.
An accepted style is required for authentic controlled casts; changing only the
combat-mode property is not a qualified style transition. The numeric-mode
fallback is restricted to synthetic fixtures, and instant server effects remain
independent. Original Update/IsInPeaceMode/AdjustMana cases cover player/NPC,
peace and non-peace styles, and low-mana clamping. MotionDone callbacks precede
Update as in WorldLandBlock: a final action already completed may release normally
before a subsequent peace check, rather than gaining a new callback-time mode gate.

Separately, durable-stage/output backpressure guards defer mode changes while a
component cost has been prepaid but its effect has not completed, a portal
operation is bound, or a terminal/fizzle output remains pending. This is ownership
hardening around atomic saves, not a GDLE-wide prohibition on windup mode changes.

Recovery belongs to the actor across attempts. GDLE's two-second streak guard is
separate from ACE's War/Void switch check, which draws a new 3–5-second threshold
per attempt. Resource retries retain their original decision. Frozen recovery
snapshots use explicit remaining times and offline elapsed time; root lifecycle
adapters preserve them across reconnect and dirty/save fences. Execution epochs
isolate deterministic cast streams across world-owner restarts; epoch zero is an
explicit synthetic fixture convention.

The component pipeline decodes the ACE DualDidMapper `0x27000002`, account formula
versions 1–3, foci/infused substitutions and per-occurrence destruction modifiers.
Requirements and sampled burns are separate. Simulation reserves all required
stacks; zero-burn reservations complete locally without an empty database write.
Burn proposals expose exact operation identity, mana and aggregate revisions.
World reserves only the caster's mana through durable resolution: unrelated
health and accepted physics keep progressing, competing mana mutations refuse,
and elapsed time cannot release the token. The matching receipt adopts mana once
before resuming the cast; definitive rollback releases it without consumption. A boolean adapter acknowledgment never
constitutes durable consumption. Final effect recovery is a new dirty mutation,
not a prediction falsely persisted as completed before the effect runs.

Connected effects include direct boosts/transfers, enchantment/dispel mutation,
life projectiles, flat bolt/streak/ring/volley/blast/wall bodies and fellowship
boost/enchantment/dispel fanout over current owner-held membership. Life source
consumption uses GDLE f32 rounding and preserves one caster health. Launch-time
leading, single bolt/arc launch and cleanup calculations are source-tested.
Connected synthetic bolt/arc tests cover hits, half-second cleanup, leading,
accepted velocity and cast-spam recovery. Rings and multishot now use GDLE's
x/y/z order, per-shot frames/velocities, self-target outward aim, radial offsets
and authored padding. The self-target ring regression launches and hits once;
624 compiled-original GDLE vectors cover 8/16 rings, fans and 3D groups.
The authored dimensions must multiply to the bounded declared count; malformed
inconsistent groups reject before spending resources or publishing any body.

Strike uses a reviewed ACE gap-fill because the selected GDLE launcher has no
strike-specific target-relative placement. Original ACE origin, target frame,
180-degree rotation and default lateral-solver methods produce 64 independent
vectors. Connected tests qualify a targeted strike impact and cleanup;
self/untargeted Strike and gravity-bearing Strike reject before resources.
Alternative ACE trajectory solver configuration is not selected here.

Projectiles remain World-owned; group admission rolls back atomically. Impact
permissions and resistance are rechecked at delivery; pet credit freezes at
launch. Prepared magic profiles now compose elemental wand damage, critical and
skill bonuses, slayer/rending, additive ratings, natural protection/vulnerability,
shield/missile absorption, augmentations and source rounding. Authentic casting
requires both damage profiles and the actual formula-component level before
motion or resource spending. Live owner registry changes feed resistance,
ratings, wand qualities and shield absorption at impact; accepted skill refresh
supplies current skills and raw Strength/Endurance. The old scalar path remains
explicitly synthetic when no full damage profile has been installed.

`oracle/damage_generate.py` compiles the original projectile scalar block,
CombatFormulas methods and TakeDamage through rounding: 1,680 combinations plus
13 formula-component rows. `oracle/procs_generate.py` independently qualifies
1,536 cloak, 72 Aetheria decisions and 13 original projectile visual intensities. The original AttributeCache getter is
compiled too: the C++ local named `maxHealth` queries current health (key 2),
and its unsigned division is retained. Cloak cast effects wait for exact item-owner
completion before damage uses the resulting vital; Aetheria requests follow
accepted impacts or resisted casts. The bounded proc queue and matching outcomes
survive output pressure; generic server output cannot steal an owned receipt.
Kernel tests exercise an actual equipped cloak healing before the pending hit,
unequipped-item rejection and once-only damage. These decision vectors do not
claim that every proc spell family is implemented.

Reviewed GDLE quirks are explicit: non-self fan rotation uses the already-updated
X when calculating Y; War bonus's skill interval uses integer division; projectile
critical setup precedes setting isPvP (PK imbue settings are therefore not used
on that path); the monster half-damage rule is upstream-labelled unconfirmed;
shield absorption has neither ACE's facing gate nor ACE's shield PvP reduction;
cloak health thresholds use integer division; the Aetheria self-target expression
has a precedence defect. Stored critical-frequency/multiplier qualities receive
the source's two enchantment passes; absent qualities receive only the explicit
pass. These preserve selected-source behavior and are not independent retail
claims. The separately reviewed negative-rating repair remains shared with combat.

Native direct harm uses the same original damage/mitigation chain; boosts retain
GDLE truncation and health ratings, and transfers retain the source vital roles,
resistances, caps and destination-full recalculation. Original transfer overdraw
is rejected atomically instead of underflowing a vital. Source-item Spellcraft,
distinct school fallback and cloak Life-projectile skill overrides have separate
compiled-original vectors. Accepted casts freeze their source skill while live
owner skill/attribute/defense refresh proceeds during windup.

Complete native damage profiles select GDLE aggregate periodic policy: base DoT
bypasses specialized defense, Nether uses live augmentation/DoT ratings, and HoT
repeats the full aggregate for the source's qualifying registry entries. Missing
casters preserve damage with explicitly absent credit. Original methods qualify
selection, call order and mitigation; synthetic legacy-only profiles retain their
separately stated ACE fixture policy. Damage/enchantment contacts retain pending
work under registry/vital/output pressure and never turn an enchantment projectile
into extra direct damage. Cloak descendants use a bounded depth16 continuation;
excess recursion fails observably and unwinds existing damage instead of deadlocking.

Cached projectile wand identity resolves current accepted item qualities and the
current wielder's aura at impact. Full profile/raw-wand refresh preserves active
cast snapshots and rejects stale raw revisions; unequipping, changing wielder and
durable item retirement are covered by a connected owner regression. Source
collision response and stock-client playback remain separate evidence gates.

The cold native factory joins the verified DAT header/formula/gestures with exact
server rows, component plans and prepared projectile geometry, then atomically
publishes definitions, flags, formula levels, target masks and shape indexes.
Actual inventory-target casts validate type masks, containment, foreign wielding,
stack limits and item resistance; geometry and PK use the actual owning actor.
Unsupported variable-dispel selection, harmful fellowship boosts and non-yaw
portal destinations fail explicitly before definition publication.
Damage immunity is not physics pass-through. The current one-impact body holds
blocked contacts pending expiry; source collision normals, post-impact gravity,
ethereal classification and default collision scripts still require integration.
This conservative contact handling is not complete GDLE collision response.
The indexed client-decompile review found that object creation applies supplied
PhysicsDesc state; its reviewed ProjectileSpell/ClientMagicSystem paths do not
provide the server ring layout. Client scripts and particle emitters control
presentation, not proof of damage geometry. The local binary version remains
unconfirmed and is not equated with AC MCP's 2005 Admin-client reconstruction.

Periodic batches cover top-category damage, Nether and healing entries, retaining
backpressure debt and applying final pulses before expiration. Nether reduction
uses GDLE augmentation/DoT resistance and its explicit configured player modifier.
Synthetic Dirty Fighting consumers retain their separately tested ACE calculation;
complete native profiles use the selected GDLE aggregate policy described above. The registry also supplies exact vitae mutation previews
for durable XP operations, preserving raw f32 values and generation-fencing the
source's delayed two-second removal.

Portal services now stage typed link/recall/sending/summon proposals over immutable
anchor copies, exact mana and participant revisions. World preflights geometry
and token-fences transit. Relocation does not imply client readiness: materializing
requires authenticated LoginComplete and server destination readiness. Original
saved anchors never receive the copied target's scale-derived Z adjustment.
Routine saves overlay the canonical link owner and preserve absent vs false flags.
Complete portal quest/provider wiring and production content/lifecycle qualification
remain necessary. Inventory targets now have explicit masks and current owner
proof; general actor/portal target policies retain their separately stated scope.
Derived attribute/skill/vital refresh is owned by the kernel's accepted registry
projection rather than by packet serialization.

`EnchantmentRegistry` owns bounded exact layers, relative active-time clocks,
caster/set/degradation metadata and transactional previews. Offline registries do
not heartbeat. Reservations retain timer debt; transfer and retirement preserve
pending output. Storage uses frozen DTOs rather than these evolving structs.
Equipped-item login audit and recovered-vitae normalization remain explicit owner
mutations, not side effects of packet serialization.

Independent evidence includes `oracle/gdle_cast.py` (77 original C++ control/mode/instant
transitions), `recovery_generate.py` (162 original ACE school-lock cases),
`life_generate.py` (60 original GDLE drain cases), `trajectory_generate.py`
(64 original GDLE launch-velocity cases), `launch_generate.py` (64 original
GDLE launch/frame cases), `lifetime_generate.py` (150 original GDLE lifetime
cases), and `components_generate.py` (original
ACE mapper parsing and foci rules over synthetic records). Existing ACE magic
wire/DAT, enchantment audit and Dirty Fighting oracles remain available. Oracle
headers record exact pins and SHA256 provenance. These fixtures establish only
the methods and adapters stated in their headers; they do not establish full
spell or motion/geometry parity.

Simulation tests are in `tests/magic*.rs` and named `src/magic/*_tests.rs` modules.
They cover resource/durability fences, recovery/reconnect, server-origin effects,
projectile ownership, periodic ordering, pet charge/spawn/retirement, portal
handshakes/anchor isolation and authoritative motion-chain completion. Run the
focused suites and required workspace checks before qualifying each integration.

Cold physical projections use `enchant_physical_quality` for GDLE's sequential
f32 scalar arithmetic and its separate f64 increasing/decreasing quality details.
`enchant_body_quality` applies undefined, base, then typed body-armor modifiers.
The accepted ACE registry winner policy remains explicit; this does not copy
GDLE's `CullEnchantmentsFromList` wildcard-bit behavior. The independent C++
oracle compiles unchanged pinned `Enchantment::Enchant`, `EnchantInt` and
`EnchantFloat` methods and verifies ten exact-bit scalar/detail vectors. Verified
DAT QualityFilter admission determines which int/float properties participate.

Physical contacts use the same native proc decisions through a bounded exact
phase receipt: weapon completion precedes live Aetheria selection, Dirty Fighting
follows the sigils, and cloak completion precedes the final health read. Unarmed
Dirty Fighting has an explicit actor-source origin; it does not fabricate an
inventory item. Contact ownership survives cancel and mode changes after admission.
The source ordering is GDLE MeleeAttackEventData.cpp (weapon/Aetheria/Dirty),
Ammunition.cpp (non-evaded missiles only), and WeenieObject.cpp TakeDamage (cloak).
Simulation phase regressions cover changed sigils after weapon completion,
malformed-count atomicity, injured-target cloak thresholds and capacity-one hits;
they complement the independent decision vectors rather than claiming complete
stock-client physical animation qualification.

Instant noncreature sources have an explicit world-object caster role. Real body
and scalar qualities supply Spellcraft and damage defaults; no combatant, account
or mana pool is fabricated. Source-vital drains reject before mutation when those
vitals do not exist. Instant Sending retains the exact portal owner completion;
its optional mana mutation permits only actual player participants in the durable
save. Resistance notices freeze source/target names and session bindings at the
accepted decision. Vital output also carries the accepted combatant incarnation,
so delayed output from an earlier session cannot cross a reconnect.

NPC registry recovery accepts exact durable entries only into fresh empty
revision-zero owners before initial effects/AI. Actor and all inventory descendant
identities are validated against the admission forest; restoration preserves
casters, durations, layers, metadata and revisions and derives physical/skill
views before publication. Noncreature registry restoration uses a separate
actual-body/property boundary. Existing live effects or queued work reject.

The original cast-control distance vectors now assert their mana/fizzle columns
as well as stopped casting. Movement at or beyond six units uses the native
five-mana fizzle, source visual intensity and successful EndCast; release checks
the skill roll first and burns no components on the successful-roll movement
branch. The owner retains an admitted resource plan across durability delays, so
it cannot reroll movement or charge a second fizzle. Normal skill fizzles emit the
native effect and explicit 1026 result. Authentic animated State/Jump integration
regressions exercise slide, displacement, stamina, cancellation and these costs.
