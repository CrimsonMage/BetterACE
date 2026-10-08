# bace-simulation

AttributeTransferDevice (63) prepares Int189/190 and all equipped
WieldRequirement slots from accepted item snapshots. Authenticated Use quotes
official confirmation type 3; Yes rechecks current ownership, item revision,
activation requirements, each equipped revision, character bounds and sequence.
The character proposal and one-item inventory take remain reserved until the
same durable operation's exact receipt is submitted. A forged or premature
receipt cannot release either owner. `tests/attribute_transfer.rs` exercises the
bounded owner and combined receipt; it is a synthetic owner test, not a client
playback claim. Generic activation effects beyond plain requirements remain held.

Status: foundation.

Default Shop Buy now has a simulation-owned reservation after accepted vendor
stock is lazy-loaded. It quotes pinned source terms, selects coin stacks in ACE
inventory order, and holds one inventory proposal behind source, stock and
marker fences. The exact joint receipt confirms it; a definite rejection
releases it. Generic inventory retry, rejection and confirmation cannot bypass
this owner. A character revision hold now fences the matching player snapshot
until the same exact joint receipt or definite rejection. Runtime Buy remains
unsupported while joined capture, fresh grant preparation, baseline adoption
and retained output are incomplete. Focused synthetic owner tests establish
this boundary without a client playback claim.

`StaffAction::Audit` admits a bounded Audit chat event through the social owner
after live Sentinel authorization. It records the authenticated sender and
current Audit subscribers, then emits one correlated staff outcome. In-game
delivery uses the existing retained social output pump; the external Audit
publisher now retains a full queue admission before assigning a feed sequence
and surfaces a closed publisher as a recoverable error. The synthetic
Audit-event test verifies the owner handoff and source channel identity.

Developer `@whoami` now reads the issuing actor's bound GUID under Developer
authority and emits its pinned one-line private response. `@myiid` remains a
separate Envoy format; owner tests check both source lines and privilege gates.

Developer `@gps` reads the issuing actor's accepted cell, position and yaw from
the world owner and emits the pinned one-line private location response. The
simulation's yaw-only pose supplies zero quaternion X/Y components; this does
not establish broader arbitrary-rotation parity or stock-client playback.

Nonpermanent landblocks enter dormant state after 30 seconds without accepted
activity and begin save-before-unload after five minutes. Both deadlines use the
30 Hz simulation clock; permanent and keep-alive regions retain their normal
residency rules. Pinned immutable content readers are released when their last
region or in-flight preparation owner drops them.

Bounded pure command queue and explicit fixed-step synthetic kernel. No I/O or wall clock in tick execution. Complete gameplay orchestration is deferred.

`Kernel` now owns registered `CharacterProgression` aggregates alongside the same
world/physics owner. Registration requires an existing world actor and unique
actor/account/session identities; failed registration returns the aggregate so
dirty state is not lost. Session IDs must encode a connection generation, not a
recycled legacy slot. `take_character` transfers owned state back to a lifecycle
adapter; it does not acknowledge a database save or authorize an early logout.

`Command::RaiseProgression` carries authenticated action correlation. Every step
rechecks the actor/account/session binding and serial action sequence before
calling the character domain. Sequence wrap uses the half-range rule; authenticated
domain rejections consume their sequence, while invalid ownership/replay does not.
This is an intentional hardening over pinned ACE's unchecked GameAction sequence.

Correlated `ProgressionOutcome` values enter a bounded, reusable outbox. When it
fills, the next progression command stays queued without consuming its sequence
or mutating character state. FIFO order is preserved and physics ticks continue.
The adapter must drain `take_progression_outcome` before submitting indefinitely;
`progression_backpressured`, pending-outcome and queued-command counts expose
pressure. Limits are explicit through `with_gameplay_limits`; `new` retains the
existing interface and uses the command capacity for outcomes, with at most 4,096
registered characters. No accepted pose/physics state is duplicated.

Integration tests cover identity replacement, stale requests, sequence wrap,
unchanged rejected state, registration limits and full-outbox backpressure.
Character expenditure itself has independent pinned ACE golden fixtures in
`bace-character`. A runtime UDP progression path and bounded result delivery are tested. Frozen save DTO composition,
complete lifecycle drains and actual stock-client gameplay remain separate work.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.

`Command::Combat` now shares authenticated binding and serial request authorization with progression. Bounded correlated outcomes retain queued commands under backpressure; bounded damage/death and completion events retain scheduled attacks until output capacity exists. Prepared melee profiles drive delayed hooks against world-owned health, rechecking accepted range and synthetic geometry at impact. Logout cancels pending attacks. Tests exercise this end-to-end synthetic path; full damage formulas, corpse loot, rewards, NPC decisions and real stock-client AC combat remain separate integration work.

The bounded synthetic population path now connects prepared NPC admission,
awareness/chase/home return, hook-timed attacks, authoritative damage/death,
ACE create-list selection, contributor reward proposals, corpse publication,
fresh-ID respawn and empty-corpse decay. `tests/pve.rs` follows that whole loop.
Existing bodies are transferred to corpses without a duplicate mutable physics
owner. Nonempty corpses retain their valuables until confirmed item transfers;
durable expiration/destruction of remaining valuables is not implemented.

A death proposal reserves online progression/logout and exposes exact expected
before/after XP/revision values. `confirm_death_committed` only accepts the same
credit projection after the adapter has durably committed corpse/items/rewards.
Errors retain pending state and retry the same proposal. The u64 operation field
is local correlation: runtime MUST scope it with a durable world generation/lease
or globally unique operation ID before submission. Full EarnXP/loot-table behavior
and production operation orchestration remain separate work. Report-only worker
shutdown must retain/recover the whole Kernel whenever `has_pve_state()` is true.

`Command::UseDoor` binds authenticated Use to `DoorAuthority` and a bounded
outcome/event channel. Prepared hook timing drives dynamic collider solidity;
accepted player/NPC occupancy defers closing and retries on the physics cadence.
Only actual swept NPC contacts can automatically open unlocked doors. `tests/doors.rs`
covers the complete synthetic path and full-outbox sequence retention. Runtime
must retain/recover all state whenever `has_door_state()` or `has_queued_door()` is
true. NPC/door tests use synthetic geometry and cannot establish stock-client
playability or authentic DAT movement. Every currently admitted synthetic Body is
a collision participant; actual content ethereal/parented flags are not yet an
admitted authentic-world path.

Review regressions in `tests/pve.rs` cover an older valuable corpse followed by a
later empty corpse, cross-cell home return with identical local coordinates,
reserved spawn IDs competing with manual NPC/door admissions, and impossible
timer profiles. Corpse maintenance selects the earliest eligible due ticket
without discarding valuables or changing their original deadlines. Home retains
both CellId and position; absent cross-cell navigation waits for the explicit,
validated home teleport rather than comparing coordinates in unrelated cells.
Queued spawn IDs and pending generator IDs remain reserved across subsystem
admissions until their owning transition completes. Blueprint/return timers are
bounded to `u32::MAX` ticks (over four years at 30 Hz), with checked deadline
arithmetic retained. This is explicit invalid-content hardening.

Skill transitions now use immutable proposals reserved by the character owner.
The exact correlated durable receipt adopts the proposal; rollback and uncertain
commit keep their distinct meanings. Skill devices are registered from trusted
Int185/186 alteration properties or Int215/Int64#3 augmentation properties.
Confirmation rechecks inventory ownership/revision, costs and all four authored
wield requirements. Missing or stale wield projections fail closed. Device use
reserves one combined character/inventory proposal and both enchantment registries;
it cannot acknowledge the skill and device consumption independently. Quoted
confirmations are bounded and expire in simulation ticks. Legacy wield-skill
conversion belongs to the prepared content adapter, never client input.
For source `SharedCooldown` and positive `CooldownDuration`, the confirmed
skill-device ticket now retains the exact player registry before revision and
after entries. The registry and its enchantment output change only after the
combined skill/item durable receipt. Active shared groups, missing registries,
and foreign item ownership reject; a cooldown-only profile reads that exact
registry without requiring unrelated combat-value projections. Other activation
response flags and emotes remain held.

Optional `PreparedCharacterSkillInputs` connect authoritative character ranks,
training class, attribute init/ranks and prepared modifiers to accepted combat
and caster projections. Valuable transitions preflight the proposed projection
before submission; accepted ordinary skill XP and durable class transitions
refresh the views. Prepared skill/attribute enchantment modifiers and equipment
must be re-prepared by their authoritative owner when those inputs change.
Registration validates the complete supplied skill set and rejects partial updates.

The current noncritical scalar melee pipeline consumes specialized melee defense,
front shield caps, attacker/defender Recklessness, and Sneak Attack using accepted
body headings. Combat event identity combines an explicitly supplied world epoch
and a checked server ordinal. Independent keyed per-hook forks drive Sneak Attack
and Dirty Fighting. Dirty Fighting queues the official trained/specialized spell
IDs; a bounded queue retains effects during registry/output pressure and blocks
logout until magic atomically admits the prepared spells. Magic projectiles use
the corresponding specialized defense and prepared shield absorption projection.
No actor task, wall clock, I/O or additional mutable character owner is introduced.

Read-only accepted projection APIs expose Healing's skill/difficulty bonus,
all three defense ratings and the physical shield cap. A complete healer use,
kit consumption/vital transaction and missile-weapon attack pipeline remain
unsupported; these projection APIs do not report healing or missile damage success.
Full physical weapon/armor/critical calculations also remain separate from the
prepared scalar melee path. These boundaries are not a stock-client parity claim.

`tests/skill_devices.rs` checks exact combined receipts, rollback, confirmation
identity/content changes, the five skill augmentations and each wield requirement
slot. `tests/skill_combat.rs` proves accepted strikes change after committed
specialization, source defense/shield caps obey accepted front/behind geometry,
ordinary skill XP refreshes consumers, and healer/missile projections preserve
training classes. Independent scalar evidence is in bace-character's 117-row
skill fixture and bace-combat's 302-row specialization fixture, generated from
verbatim official ACE at the pin in docs/baselines.toml, including
SkillAlterationDevice/Player_Skills, CreatureSkill, Creature_Rating,
Creature_Combat, Player_Combat, SkillFormula, Healer and SpellProjectile.

## Native gameplay integration additions

Native NPC action execution now uses bounded owner proposals and exact completion
receipts. `tests/native_npc.rs` observes real property, quest, XP and luminance
state, branch-dependent speech, malformed receipt rejection and notification
backpressure. Give-category entry requires the source-defined accepted input
consumption stage; it does not combine every authored action into one transaction. Delegated services remain pending/unsupported until their actual
owner can complete them; numeric interpretation alone is not gameplay completion.

Prepared NPC scripts now register during actual actor admission, with the same
immutable identity handed to runtime. A distinct World admission hold prevents
interaction/targeting while gravity can settle; exact definition binding and
recovery readiness release it. Source journal inventory holds are separate from
region/death reservations and retain full registry images until an exact receipt.
Source metadata can be discarded at shutdown only after actors, VM work, durable
stages and all outward evidence drain.

Native death loot uses a frozen event ID, key version, graph/content-generation
hashes, immutable policy and complete generated item mutations. Ordinary graph
rolls never read or advance a character's rare stream. Eligible rare outcomes,
including failures, reserve the credited character's exact before/after state
with the XP proposal. A single post-revision covers both, including zero-XP rare
checks. Respawns require a fresh supplied event context; queued IDs can receive
that context without losing their reservation. The old injected f32 queue is an
explicit synthetic fixture path and is refused after native RNG configuration.
Native failures remain observable through `native_loot_failure()`.

`OwnedCharacterState` lifecycle transfer retains rare state (and installed UI
state). The progression-only transfer refuses instead of dropping/reseeding it.
Runtime's native-loot regressions verify frozen hashes/mutations, top-damager
attribution despite another player's final blow, unchanged other-player rare
state, retries and receipt-gated advancement. Missing native profiles/states do
not trigger a fallback to shared randomness.

Housing payments use `prepare_housing_payment` to bind the admitted house ticket
to exact-ID inventory consumption. `confirm_housing_payment` validates both
receipts before mutating either owner. Standalone paid-house confirmation is
rejected. `tests/housing_payment.rs` covers wrong receipts, joint adoption and
confirmed rollback. The caller must commit house/player/payment rows together;
this local confirmation API does not pretend to be a database commit. Nonzero
currency change requires a prepared combined take/grant operation and is refused
by this narrower exact-payment entry point.

Magic and inventory have their own bounded state/outcome/ticket channels on this
same kernel owner. Accepted resource changes are prevalidated as a complete
batch, projectiles have world-owned physics state, and magic damage enters the
same death/loot path. Other subsystem reservations block conflicting progression,
UI, logout and valuable operations. Synthetic-scene integration remains distinct
from authentic-world/stock-client readiness and from unimplemented service paths.

Generic inventory proposals freeze registries for their exact participants,
including siblings whose container slots compact, and the owning character.
Housing payment and skill-device composites join their existing reservations
without releasing another owner's locks. Removed item registries retire only
after the matching durable tombstone receipt; rollback retains the complete item
and applies owed active time after release. Other owners' items continue aging.
`tests/inventory_magic.rs`, `tests/housing_registry.rs` and the device registry
regression exercise these cases across a ten-second pending receipt.

Dirty Fighting's current-skill additive is composed from the live registry at
refresh, so prepared skill additives must exclude that separately owned effect.
The cached aggregate/registry revisions skip unchanged projections; bounded
reused scratch detects registry changes, and timer-only revisions with identical
Dirty Fighting modifiers advance the marker without rebuilding skill values.
Busy casts retain the prior marker until the owner can refresh safely.

The fixed tick currently resolves combat before the magic heartbeat. A strike on
an expiry boundary uses the preceding projection; expiry is visible by the end
of that completed tick. `tests/skill_registry_refresh.rs` checks that boundary,
current-only modifiers, and rejection of premature public registry release.

Accepted player positions and vital pools now produce immutable save projections
on this same world owner. Registration seeds the baseline without creating a
mutation. Later changes retain the original dirty age through repeated updates,
failed saves and valuable-operation reservations. A matching old completion
cannot clear a newer accepted state. Cast-recovery revision adoption also waits
for aggregate reservations to finish; logout transfers recovery and portal links
with the complete player state. Actual runtime scheduling and stock-client
lifecycle composition remain separate gates.

Cast gesture completion currently comes from a server-owned prepared duration.
The cast driver rejects stale correlated completions and uses accepted world
observations for turning, displacement and release eligibility. This is not yet
a claim of complete DAT-driven GDLE transition-chain execution; that connection
and stock-client playback qualification remain in progress.

Generators use the existing world, population and inventory owners. Their domain
machine is `bace-spawning`; the kernel validates exact incarnation/revision keys,
reserves dynamic IDs and atomically adopts physical placement with membership.
Inventory acquisitions reserve generated descendants before persistence; resets
and unloads retain lifecycle effects while those operations are pending. Durable
world remnants and mixed sibling compaction use explicit retirement proposals.
Vendor contributions refer to retained stock IDs after source-style merges, with
precise withdrawal and identity reservation. Bounded generator command outcomes
make adapter acceptance separate from gameplay success.

Native NoShareExperience now uses a two-stage durable continuation. Admission
freezes the source-scaled amount in a workflow-only journal entry, then detaches
the recipient work and lets immediate emote rows run. The recipient prepares its
actual XP/level/credit proposal afterward from current owner state. Both Ready
queue recovery and adopted-recipient recovery remain blocked until loaded-owner
readiness is acknowledged. `tests/native_npc.rs` compares zero-delay query and
intervening-property ordering against the compiled official ACE queue fixture,
checks explicit unsupported shared rewards, and rejects duplicate receipts.
Current gates require empty equipped inventory, no vitae, non-Olthoi, and already
full vitals when leveling; missing sharing/item/vitae/allegiance and vital-restoration
stages are not simulated as success. See bace-emotes for source and harness scope.

Player death and command recalls have dedicated bounded owners. A death holds the
player and complete owned inventory before cold preparation; an immutable ticket
contains loss selection, corpse placement, post-purge registry, vitae metadata,
respawn position and recalculated vital maxima. Only an exact atomic receipt starts
the source animation and delayed return. The durable checkpoint is deliberately the
final alive state: a crash after commitment restores the committed item loss and
alive player rather than replaying selection or restoring zero health. Pending
receipts and temporarily blocked stages remain retained; `player_death_blocked`
exposes the last retry obstruction. Death item RNG is keyed by world epoch,
operation and character. Lost equipped-item auras and gear health are excluded from
respawn maxima. Shared XP composes item-set registry changes before Vitae recovery
and preflights their combined output budget.
Olthoi's separate death selection leaves player possessions in place and admits
generated container descendants with source parent and final slot order in the
same inventory proposal. The corpse's direct-child roster contains roots only;
descendants remain owned by those roots through expiry and spill.
Corpse consent grants belong to the online recipient's transient simulation
state. Authenticated add/revoke/list/clear/remove commands use the source
one-hour expiry and the 20-person list bound documented in pinned ACE; grants retain the granter's name for
offline display/removal and are discarded on recipient logout. Corpse Inspect
derives the permit from that owner at an explicit adapter-supplied wall time;
only a committed Open consumes it and records the durable one-corpse permittee.

Command recalls read current housing/allegiance permissions, prepare destination
geometry before debiting mana, and recheck source movement/permissions at completion.
They reuse the durable portal owner and authenticated ready/materialization gate.
Binding updates use the same atomic player/allegiance saves. Cold DAT animation
preparation and event-to-client projection are adapter responsibilities; the typed
`Started` event alone is not a stock-client animation compatibility claim.

Prepared command recalls now admit verified DAT chains to the existing world
motion owner under a dedicated recall token. Revision, action, stance and resource
checks precede admission and mana debit. Animation completion is independent of
the source timer, so Marketplace can stage at phase 420 while its longer action
is unfinished. Cancellation and actor epoch changes retire pending recalls;
callbacks remain bounded and drained after durable staging. Three synthetic
authority regressions cover stale/wrong programs, cancellation, the fourteen-second
boundary and read-only destination inspection without RNG advancement. Pinned ACE
`Player_Location`/`MotionTable` timing vectors and actual-DAT cold-chain tests live
in `bace-runtime`; these owner tests alone do not prove output parity. Full live
command ingress, combat stance transitions and canonical recall output remain
adapter integration work.

World residency retains epoch-fenced preparation, activity, dormancy and draining
states. Dormancy stops physics/NPC thinking while generators remain scheduled.
Unload acknowledgement requires actual owner cleanup; emitting `Unload` alone
never authorizes discarding unsaved world items. The region-residency integration
tests cover timers, bounded backpressure and stale epochs. Scalar policy provenance
and compiled-original fixtures live in `bace-interactions/oracle/ace_world_policy`;
player-death owner tests cover exact receipts, reservations, lost equipment and
protection/PK heartbeats.

Item XP rewards now join the same reserved inventory/player operation as earned
XP. Prepared item identity, accepted equipment order, XP curve and DAT set tiers
are explicit; unknown equipped metadata rejects preparation. Set level crossings
prepare exact registry candidates, and only the matching durable receipt adopts
XP, item revisions and set effects. Vitae composition applies after the item set
candidate. Rejections release the exact reservations; uncertain writes retain
unchanged accepted state. Item XP publication and logout ownership are bounded.
Focused tests cover receipt mismatch, rollback, nonlevelable equipment and set
spell replacement. This does not claim a completed login/equipment host adapter.

Complete resident admission now joins prepared static/encounter roots with durable
world-item forests, all item registries and explicit corpse deadlines. It stages
only the bounded incoming inventory graph, validates every owner before adopting
geometry, and returns the whole input on rejection. Initial generators can run
only after that single transition. Dormant regions stop active bodies/NPC thinking;
draining regions stop generators, retire projectile owners and await exact captured
world-tree saves. The final `Unloaded` event follows actual geometry/owner eviction,
so runtime asset/source caches remain available until teardown finishes.

Generated forest admission validates bounded ancestry and exact request GUIDs before
world mutation. Failed placement discards only the failed root and its descendants;
only roots contribute generator membership. `oracle/generator_spawn.py` compiles the
unchanged pinned ACE `GeneratorProfile.Spawn` object loop with controlled placement
results; `tests/generated_inventory/trees.rs` compares retained roots with its golden
fixture and tests malformed-tree rejection. Direct correlated request refreshes
preserve RNG and allocated identities without consuming another shared request slot.
Generated retirement captures accepted world poses and bounded live enchantment rows
before durable destruction; surviving container siblings retain their current rows.

Generated mixed creature/item roots now enter through one source-ordered bounded
transition. It preflights only incoming immutable inventory graphs, rolls back
fresh bodies/combat/loot contexts on non-placement errors, and reports complete
accepted root trees in one receipt. A placement failure omits that root and its
children while preserving other successful roots. Multiple creature roots retain
distinct death-event identities. Synthetic regression tests cover success,
rollback/retry and partial placement; they do not qualify actual DAT or clients.

Both static and generated ACE creature admission consume the source monster-state
predicate: non-attackable creatures with no targeting tactic do not acquire combat
targets. This preserves passive Vendor actors without inventing hostility; their
stock owner remains separate from physical/creature ownership. The unchanged
pinned `Creature.SetMonsterState` method supplies six golden cases in
`tests/fixtures/creature_combat_ai.csv`, generated alongside the runtime factory
oracle. GamePiece actors, Shop creatures and specialized/mixed Contain creature
forests remain explicitly unsupported; ordinary single Contain construction is
described below.

Staff startup effects are admitted atomically to a dedicated direct-effect index.
They retain cold source aura/set classification and do not occupy native player
casting IDs. The immutable player snapshot now includes the current registered
staff binding/privileges for runtime account/shard authorization. Tests reject
partial staff catalogs and prove native casting definitions remain independently
admissible after direct staff effects are installed.

Pinned `AdminCommands.HandleHeal` restores a selected player only. Missing
landblock targets and selected nonplayers emit its source Broadcast chat from
the bounded staff event owner; no nonplayer vital mutation occurs. A copied
accepted visibility name supplies the nonplayer text, while the current world
still decides target scope and type. The staff integration regression checks both
rejections and the player vital path.

Pinned `DeveloperCommands.HandleMyLoc` has a distinct read-only staff action.
The owner reads accepted world cell, origin, and yaw and emits the source
landblock, LOC, and physics lines in order. The live world stores yaw rather
than an arbitrary full quaternion, so this is an authoritative yaw projection,
not a claim of universal source position formatting. `@targetloc` remains a
separate selected-target route.

Native cold casting programs are admitted per actor with an exact live binding
and captured character revision. Shared effect/flag/target metadata must agree
for the same spell ID; components and verified gesture chains remain actor-owned.
Once an ID uses this admission, both player and server-origin casts reject a
missing actor program rather than using another account's global program. Atomic
rejection and two-account isolation are covered by the actor_program regression.
Portal completion retains the exact attempt on driver/recovery failure and records
release once; a retry cannot change its accepted result or replay the portal effect.


Portal tickets now hold every character participant under a distinct operation
namespace until exact rejection or committed effect adoption. UI, XP and routine
captures cannot advance their before revisions during a save. The bounded
`PortalResolution` command/outcome lane distinguishes admission from owner
acceptance. Extended-save regressions verify both successful and rejected recalls
release casting recovery and permit another cast. Magic completion remains held
across registry timer debt; retry cannot repeat a teleport or lose its attempt.
These are durability regressions around the pinned ACE portal source behavior,
not client qualification.

Equipment effects are frozen companions to the exact inventory operation: ordered
set deltas, item mana activation, source spell filtering, item XP membership and
intermediate vital clamps are prepared without mutating accepted state. Inventory
owns their final ordered publication; generic magic output does not duplicate it.
The pinned activation and equipment-set methods have 48 and 28 independently
compiled C# vectors in `oracle/equipment_activation.py` and `oracle/equipment_sets.py`.

The equipment mana owner implements pinned `Player_Tick.ManaConsumersTick`, including
float accumulator arithmetic, the player's cached heartbeat interval, rating-based
burn reduction, one low-mana warning, and the two-second depletion removal. Its
1,080 compiled C# vectors cover exact float bits and warning/depletion boundaries.
Initial heartbeat phase uses the source 0–5 second spread with BetterACE's keyed
per-session purpose stream. Valuable holds retain due work and transient fractions;
output pressure leaves the whole heartbeat unchanged. Queues and metadata are
bounded, and recurring work reuses scratch buffers. Biota mana and IsAffecting are
saved at the accepted item revision; source transient accumulator/warning/action
state stays with the live owner and is reset only on cold reconstruction.

Durability hardening: ACE does not persist its delayed depletion action. A cold
zero-mana, non-affecting item with a matching known item-cast enchantment schedules
an immediate exact-caster cleanup before entry capture. It preserves unrelated
casters/spells and does not replay messages or sounds. Normal pending depletion
allows routine and correlated valuable snapshots, retaining the live obligation;
logout waits for removal and retained notices. This prevents a delayed action from
being silently lost or blocking its own valuable-operation capture.

A bounded constructed-Creature owner now retains ordinary Creature/Cow (factory types
10/15) Contain roots before world entry. Inventory owns the complete carried/equipped
graph; Magic owns the same delayed 0.1-second item spells and registries. Immutable
NPC preparation and child identities are bounded to 128 roots/4,096 total items.
Trusted transient promotion validates the root revision, full graph, reservations,
geometry and physical profile, then moves that same identity into the existing NPC
owner. Placement failure preserves construction. Exact registry rows and the
remaining spell deadline survive promotion; they are not recreated.

First valuable transfer of constructed Creature/Cow roots now uses a dedicated
world-fenced graph promotion receipt and subtype-aware V4 construction companion;
ordinary item placement cannot create a fresh constructed creature. Nested
creature-valued Contain trees retain each creature as a distinct cold simulation
owner with its exact equipment order, enchantments and death roster. Promotion
still waits if source slot compaction needs a separate companion. Generator teardown
removes the retained construction; a detach-only unload stays visibly blocked
instead of serializing an incomplete generic item. Specialized Creature subtypes,
Shop commerce, nested generators and DAT-backed composed recursive admission
remain unsupported or unqualified as documented in the runtime README.
Multiple mixed Creature/Cow and item Contain roots now preflight and adopt one
complete forest with one source-ordered receipt; rejected roots leave the whole
occurrence and immutable inputs available for retry. `oracle/generator_destinations.py` compiles the unchanged ACE
Spawn_Container/Spawn_Shop methods into 12 subtype-dispatch cases; inventory acceptance
is stubbed and is not a claim of complete container/vendor parity. The constructed
owner tests cover delayed effects, failed/exact promotion, graph rejection, durable
transfer gates and teardown. Existing physical aura-consumption limitations remain.

Health/item-mana queries now update accepted ephemeral selection fields on the
simulation owner. Source zero clears, missing-target behavior, current/adjacent
loaded outdoor landblock scope, optional item mana and selected health callbacks
are represented explicitly. Health changes retain the original subscribed
binding and accepted pool through output pressure. `oracle/target_query` runs the
original Player query and Creature subscription/callback methods: 84 query cases,
42 admitted-positive-health cases compared by the owner, and two original
intermediate callback outputs. Zero-health-maximum objects remain inadmissible;
the wire oracle separately preserves source IEEE-754 payloads. This does not
implement appraisal/IdentifyObject or claim complete selected-command coverage.

The NPC population owner now freezes authored Tolerance Int67 at admission.
An Idle creature that permits Appraise wake can preview then adopt one
examiner-targeted wake, retaining source Scream and NewEnemy obligations in
order and the examiner target across the next AI scan. Focused owner tests
cover tolerance exclusion, single wake and target retention. Live Identify
remains unsupported until its filtered profile, friendly alerts, authored
emotes and reliable publication form one retained action.

Creature-origin mana is charged once before its first motion when AIUsesMana
permits it; otherwise neither phase charges. The pinned GDLE
`SpellcastingManager.cpp` `CreatureBeginCast` (2949–2998) and `LaunchSpellEffect`
(858–930), commit `353cbab52ef7da2b7063bc3e3f008461d8531693`, charge a
player-owned noninstant emote twice because the release branch checks actual
`AsPlayer()` rather than cast origin. There is no `IsCreatureCast` guard at that
pin. BetterACE intentionally restricts release mana conversion, fizzle and costs
to `CastOrigin::Player`; Monster and noninstant Emote casts retain their single
up-front base cost when enabled, including when that empties the pool. Pinned
GDLE also charges a player-emote at release when AIUsesMana disables admission
cost; the explicit-origin policy intentionally removes that charge too. The
oracle records this case; connected owner comparisons cover enabled mana. This correction is
recorded as `gdle-player-emote-double-mana` in `docs/divergences.toml`.
`oracle/creature_mana.py` compiles the original admission method and release
resource prefix into 1,512 discrepancy vectors; the callback regression compares
the normal-NPC and player-emote boundary rows. It stubs spell validity, skill,
components and generated mana cost; it does not qualify those formulas or full
spell effects. `tests/magic_callbacks.rs` separately exercises accepted bodies
and authoritative motion callbacks for NPCs, player emotes and ordinary player
casts. Client testing remains deferred.

Ordinary region/generated removal now retires an idle NPC script only after its
actual body removal and after source continuations, journals, held inventory and
outward NPC evidence drain. Detached DeleteSelf archives remain owned separately.
New Use/hand-in/signal admissions respect a source inventory hold, so a pending
unload cannot acquire a new script obligation after freezing its item forest.
