# bace-runtime

Sentinel `@boot` now resolves the captured authorized command against online
session generations. It retains the issuer's source Broadcast response, queues
the target's final AccountBoot packet before disconnect, and keeps the Audit
feed obligation until the configured publisher accepts it. A missing target or
invalid selector receives the pinned source error without terminating anyone.
Source: `SentinelCommands.HandleBoot` at the ACE pin in `docs/baselines.toml`.
Focused typed-dispatch and syntax tests use synthetic identities; stock
client playback remains unqualified.

Developer `@whoami` now routes through the authenticated staff inspection
owner and private Broadcast response. It reports only the issuer's accepted
GUID, with the pinned `ObjectGuid.ToString()` eight-digit hexadecimal form.
Parser, typed dispatch and simulation owner tests cover this single handler;
stock-client playback remains unqualified.

Developer `@listplayers` now snapshots authenticated entered sessions and
returns the pinned access-filter header, `Name : AccountId` lines and connected
total as private Broadcast output. The issuer binding and Developer authority
are rechecked at execution. Output is bounded and divided at complete lines;
the host Console route and stock-client playback remain unqualified. Source:
`DeveloperCommands.HandleListPlayers` at the ACE pin in `docs/baselines.toml`.

Developer `@gps` now uses the existing authenticated staff inspection route to
read accepted world pose and project the pinned private Broadcast line. Parser,
typed dispatch and simulation owner tests cover it; stock-client playback is
unqualified.

Sentinel `@ban` and `@unban` now use the account-ban CAS journal with a stable
operation ID. Uncertain commits retry the exact request; private success, the
online account boot and in-game Audit emission wait for the durable receipt.
`@banlist` pages active accounts by canonical name and formats ACE's server-local
expiry and retained reason. A replaced target session generation cannot receive
the old boot. If the issuer disconnects after Audit admission, no private packet
is sent; the valid target boot still proceeds. An issuer detached before Audit
admission leaves the post-commit Audit obligation retained as a failure because
the simulation cannot reauthorize that stale context. The external Audit feed is
now retained on a full publication queue before any in-game recipient advances;
a closed publisher surfaces a degraded error with the accepted event recoverable.
General and Trade retain their existing visible drop behavior. These
paths have focused parser, PostgreSQL receipt, simulation Audit and disconnect
tests; stock-client command playback is still unqualified.

Attribute-transfer devices have a separate type-63 Use/Yes route. Accepted
Int189/190 properties and all four equipped Attrib/RawAttrib requirement slots
are prepared before simulation admission. Pinned ACE confirmation type 3 asks
about 10 points; source rules can transfer fewer near the 10/100 bounds. The
shared critical save service freezes two innate values and one item change into
one revision-fenced placement operation, retries uncertain outcomes with the same
bytes, and projects two private attribute updates, WeenieError 0x04E1, then
consumption only after the exact receipt. Pinned ACE's success opcode differs
from the user-supplied retail UseDone observation in divergence 40. Source:
`AttributeTransferDevice.ActOnUse/VerifyRequirements` and
`Confirmation_AlterAttribute` at the ACE pin in `docs/baselines.toml`. The
source's to-high error text names the from attribute; the output fixture keeps
that discrepancy explicit. Authored cooldowns, emotes, activation targets and
additional responses remain held by the generic activation owner. Domain,
simulation, session, replication and frozen-save tests use synthetic inputs;
stock-client playback remains deferred.

Disconnected portal sessions transfer their pending completion through the
existing correlated player-detach owner after committed private and observer
output obligations drain. They do not synthesize a client readiness packet or a
materialization event. Exact operation, accepted epoch/view, binding and detach
correlation remain retained until the owner receipt; a rejected detach preserves
the transit for retry. This is lifecycle hardening, covered by retained handoff
tests and a simulation recall-to-detach regression, not a stock-client claim.

Summoned portal visibility now cold-loads the accepted `portalgateway` template
(WCID 1955) that pinned ACE constructs, while retaining the linked original
portal in the immutable ticket and simulation policy. A two-template accepted
pack regression checks this distinction; client appearance is unqualified.

Status: foundation.

Thin application composition plus working content publication and fair dedicated save workers. `serve` starts the composed game runtime with accepted content and verified assets; stock-client gameplay/world orchestration remains unqualified.

Running `bace-server` or `bace-server serve` without `--config` creates a
`server.toml` template in the current working directory on first run. Later
runs load that file without changing it. An explicit `--config FILE` must
already exist. The template names conventional DAT, accepted pack, and private
RNG key paths; the resources still require provisioning, and the existing
game-readiness checks still apply. A passwordless local PostgreSQL URL can be
set in `database_url`; `database_url_env` takes precedence when present.
Direct `serve` reports the retained lifecycle reason while graceful drain is
in progress, and keeps the world owner until durable shutdown completes.

`simulation::SimulationWorker` moves the kernel onto one named OS thread; physics and world share that owner. Bounded nonblocking command admission, bounded work per tick, explicit shutdown/join, 30 Hz pacing and fixed-size timing histograms keep adapter work separate. `exercise` uses this worker unpaced. A rejected command is currently counted, not routed to a stock-client correction packet. Worker shutdown does not itself integrate the save coordinator: durable drain composition remains part of the playable milestone.

The content harness uses two asynchronous runtime workers. Saves use a separately reserved PostgreSQL writer connection with bounded operation deadlines; they do not run on the simulation thread.

Simulation submission acknowledges queue admission only. Closure rejects new submissions. The legacy report field `discarded_commands` counts unapplied adapter inputs; recoverable shutdown returns those inputs with the kernel. Calling `wait` on an unlimited worker returns an error after stopping and joining it, so the stop handle cannot be lost in an endless wait. Tests cover queue-full ownership, continuous input, finite/explicit shutdown accounting and a blocked save adapter while ticks progress.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.


Networking foundation now includes a dedicated `NetworkThread` owning the paired
UDP adapters, bounded peer drivers, generation-fenced sessions, account replacement
and explicit owner-drain acknowledgments. The API has real loopback handshake and
reliable-message tests. `try_send` acknowledges queue admission only; callers must
handle `CommandRejected` and `Terminated`, and retain ownership until drain. Invalid
packets do not refresh liveness. A byte-identical challenge is retained for bounded
retries with matching credential identity, intentionally differing from ACE's
remove-on-repeated-login behavior. No reliable traffic is emitted before cookie proof.

`AuthenticationPool` has a fixed worker count, bounded work/completion capacity and
blocking Argon2 execution. Auto-creation is enabled by default through AccountConfig.
Graceful drain returns completed and unresolved session identities, including worker
panic information, and waits for blocking password jobs even after request timeout.
Session generations fence all asynchronous outcomes.
After password proof, the same bounded authentication worker obtains the
durable account-ban verdict. An expired ban is cleared with a stable exact
operation before entry; a current ban completes the cookie challenge, sends
AccountBanned on the reliable game queue, then closes without admitting the
account. Focused fake-repository and UDP loopback tests cover these paths;
stock-client login remains unqualified.

`DatPreparationWorker` owns archive reads and compression on separate bounded
blocking capacity. Rejected jobs retain caller ownership; preparation results are
fenced against the DDD generation. Explicit shutdown recovers accepted results and
unrecovered jobs. Dropping the worker counts abandoned results, never DDD success.
Fingerprint validation/catalog production must occur before composition.

These APIs are integration building blocks. The composed `serve` path now owns
character persistence, world/asset admission, gameplay routing and production
session/DDD composition for its implemented paths; complete client qualification
remains open. Network shutdown alone does not
claim that simulation or persistence has drained. Linux loopback tests are not
Windows/macOS or stock-client qualification. Network idle polling currently sleeps
1 ms and processes bounded rotating batches; shard capacity is not performance-qualified.


The simulation adapter now has a bounded correlated progression-result channel.
A stalled result consumer retains the overflow result and backpressures commands;
physics keeps ticking. `Kernel::try_enqueue` returns rejected command ownership,
so adapter admission cannot silently lose a command when the kernel is full.
`shutdown_recover` / `wait_recover` return the single kernel owner, unapplied inputs,
undelivered channel results and any tick failure. Kernel outbox results remain in
the returned kernel. Report-only exit refuses to discard characters or progression
results and returns `WorkerError::RecoveryRequired` carrying that state. Invalid
startup configuration or OS thread creation failure also returns kernel ownership.
These transfers are not persistence acknowledgments; Drop is not a graceful save drain.

`character_assets::prepare_character_assets` turns verified decoded XP/skill/CharGen
inputs into shared domain tables and heritage allocation rules, retaining all original
data. It does no I/O and supplies no synthetic fallback. Tests prepared the pinned
user-supplied portal archive (38 skills, 13 heritages); complete character lifecycle,
appearance instantiation and durable saves remain pending.

`PortalClock` separates the explicitly supplied world-time origin from monotonic
reliability deadlines and wraps the packet header's 16-bit time field correctly.
The stock-client time range from pinned DerethDateTime is checked; invalid origins
or exhaustion fail explicitly. Zero remains the synthetic harness default. Real
world composition must supply its authoritative portal-year origin; client echoes
cannot change it. This does not implement `@settime` or claim that the reported
other-shard time-change crash is resolved.


The expanded save worker routes fenced inventory and housing operations through
its bounded critical lane; offline character snapshots share routine deadline
scheduling. Tests prove overdue offline snapshots receive service under inventory
transaction pressure. A save ticket remains a receipt to reconcile, not permission
to drop the authoritative aggregate on failure.

`PackIoWorker` owns bounded runtime pack open/write/compaction jobs on one OS thread.
It returns original jobs on failure, enforces one base plus at most two deltas,
and cancels compaction between records when save pressure returns. It never accepts
a database head. Reader-safe automatic reclamation remains uncomposed.
`WorldContentWorker` separately prepares bounded regional instances, encounters,
linked-instance references and shared immutable templates. Missing required content
fails the whole preparation. This does not admit geometry or spawn live actors.

`serve` inspects PostgreSQL's accepted manifest, reopens mapped indexes and
starts the game socket after verified DAT preparation. The separate `check`
inventory reports local DAT presence; presence alone is not fingerprint or
collision qualification.


Routine `OwnedSaveBatch` submissions use the reserved age/deadline lane, keeping
clean ancestor snapshots out of writes while still checking their ownership fences.
`GameLoginService` returns the complete bounded decoded possession tree or retains
the Loading fence on failure. Combat, death-proposal, PvE and door channels preserve
admitted output during backpressure and recover it with the kernel on shutdown.
The server-side service loop and authentic asset admission are connected for
implemented paths; complete stock-client world-entry evidence is still missing.

`native_publication::publish_native_once` validates native loot/rare identities and
referenced item templates, reserves bounded delivery, and uses `PackIoWorker` for
immutable delta creation/compaction before journalled mapped acceptance. Save
pressure defers compaction. It maintains one base plus at most two active deltas;
retired reader-held files are not automatically reclaimed. Mixed weenie/profile
SQL transactions are accepted as one bounded publication. The host console's
explicit [reviewed inbox](../../../docs/content-inbox.md) stages weenies, world
rows, ClothingBase records and tombstones into that same journal. The
host queues reviewed changes while running; the game child owns publication and
adopts accepted generations for future region inputs. Accepted delta keys now
select occupied regions whose authored indexes or template closures changed.
A bounded off-thread preparation and exact epoch/revision/manifest owner receipt
can add and remove plain static roots with reliable create/delete visibility;
existing actors and durable world trees retain their admitted revisions. Unchanged
plain regions skip DAT preparation. Complex NPC/generator root changes, structural
generator-profile migration and ClothingBase refresh remain held with a degraded
region diagnostic. An unrelated later publication does not clear that diagnostic;
a targeted correction or explicit retry prepares against current durable content.
Refresh failure cannot block save-before-unload. The approved-DAT occupied-client
publication replay is an explicit validation gate, not a current playability claim.
After task
cancellation or uncertain database commit, discard that worker and reload the
accepted manifest before resuming; do not reuse an in-flight completion blindly.

`death_saves::freeze_native_death` combines the frozen native death's generated
mutations, one shared corpse, spendable-XP proposals and per-character rare-state
advance into one placement operation with a stable event-derived operation ID.
It preserves the latest UI/enchantment save supplements. This is an atomic save
adapter, not complete ACE EarnXP level/fellowship/allegiance processing or a
running stock-client world loop.


Character feature adapters now preserve schema3 UI/enchantment supplements while
loading and freezing progression, inventory, corpses and housing. `player_saves`
composes sibling supplements against one prior revision, rejecting same-revision
mutation. `skill_saves` freezes plain training or combined skill/device consumption
with owner leases, exact CAS and caller-allocated durable operation IDs. Its private
reply receiver validates acknowledgments; uncertainty remains sticky until the
same operation resolves successfully. Crafting freezes source/target/player and
all slot-compacted siblings together. The simulation worker has bounded skill,
UI, crafting and device lanes and returns undelivered results during recovery.
The direct skill/device, crafting/salvage, and enchantment item writers now
freeze `ItemSaveV5`; they retain each existing item's immutable source
`DestinationType` while fresh salvage bags keep an unknown origin. Legacy V4
enchantment reads remain supported during cold preparation.

`game_random::load_and_bind` reads a provisioned private key on blocking capacity
and binds its immutable fingerprint before `BoundGameplayRandom::configure_kernel`
installs shared native-loot, crafting, magic and combat roots. `random_key_file`
must be configured for gameplay readiness; missing files are never regenerated on
restart. NPC composition uses the same bound root through its prepared service API.
These tested adapter paths do not remove the existing stock-client service gates.

Player schema 4 carries an explicitly versioned cast-recovery supplement and
contract states. `restore_player_at` requires an explicit trusted clock when
recovery exists; elapsed offline time ages the remaining recovery without
resetting it. `freeze_player_state` composes the drained player, accepted world
pose/vitals, portal links, registries and recovery. These adapters are not yet a
complete online routine-save/lifecycle service. `tests/world_saves.rs` checks
binary reload, preservation of unrelated fields and atomic rejection of invalid
projections. Simulation's `player_world_saves` regressions check original dirty
age, stale/failed completions and mana/vital transfer at logout.

Pet events, portal proposals/events, physical launches/events and server-origin
cast outcomes use bounded simulation output lanes. Recoverable shutdown returns
channel output before overflow and retains remaining kernel work. Queue admission
is never a durable receipt, portal completion or spell-effect acknowledgment.

`npc_persistence::freeze_player_stage` combines a prepared character effect with
the simulation's adopted-marker checkpoint in one journaled NPC stage operation.
It preserves the exact operation bytes across uncertain responses and releases
only the matching receipt. This adapter covers typed player property, quest,
XP/luminance, title/sanctuary, contract and recipient-level changes; other named
services require their own owner adapters. A completed earlier stage remains
committed when a later stage fails. The bounded production NPC coordinator loads
canonical source heads and participant owners before acknowledging recovery.
Cold static/generated registration carries the exact admitted program identity;
script sources remain unavailable for targeting until the correlated Ready reply.
The source journal includes live scalar/MyQuest state, accepted cell/pose, and
exact held source/gear enchantments and inventory identities. Source aggregates,
versioned item gear rows, custody and the continuation commit in one transaction.
Recovery uses current durable custody, including later transfers/tombstones;
registry revisions rebase to zero in a new world incarnation while SQL versions
and item mutation revisions remain the durable fences. Old in-memory registry
proposals are not reused. Historical generator-parent membership, persistent generic-NPC gear destruction
and unsupported named effects remain explicit held work. Focused tests cover snapshots and
receipt uncertainty; PostgreSQL tests cover atomic source/gear/head writes,
replay, CAS, location lookup, later tombstones and world-execution fencing.

## Generator preparation and durable ownership

`region_activation`, `generator_preparation`, `treasure_assets`, `generator_treasure`
and `generator_items` prepare immutable accepted content off the simulation thread.
Region activation carries actual DAT geometry and source roots in a single owner
command. Missing or mismatched assets produce held diagnostics. Vendor generation
is off by default; generator rotation offsets are on. Treasure lookup resolves
ACE death before wielded tables, using the overloaded source WCID namespace.
Material rows retain imported ID order (the upstream database query has no explicit
ordering); clothing fallback preserves the original DAT hash-table order.

`SimulationWorker` has bounded generator request, event, correlated outcome and
retirement lanes. Undelivered output and the original kernel survive recoverable
shutdown. Asset callers retain their materialized collection, reserve additional
IDs for a multi-item result, then submit the exact-key admission command. An empty
treasure result completes with an empty materialized spawn receipt.

`generated_saves` freezes first acquisitions, including transient descendants,
into one epoch-fenced placement operation. `generated_retirement` does the same
for tombstones and slot compaction when an object or sibling already has durable
state. Private save-worker replies bind the original request; uncertainty retains
reservations and retries the same durable operation ID. A receipt acknowledges the
whole proposal before generator membership or accepted inventory changes.
Transient populations rebuild from accepted content; acquired items use ordinary
versioned saves. These adapters do not remove the main stock-client readiness gate.

Generated stack identity excludes quantity, totals, placement and authoring metadata.
It retains intrinsic property differences as an explicit hardening of ACE's WCID-only
merge check. Stackable items use source zero defaults for missing unit burden/value;
nonstack items retain their total values. The current inventory projection supports
Unique limits zero/one and rejects larger limits explicitly. Player-dependent quest
and wield eligibility still belongs to action admission, not spawn-time guessing.
Cold `generator_catalog` joins required DID32/33/35 and generator treasure rows,
retains source row order, and expands only the bounded referenced template closure.

`generated_recovery` classifies bounded persisted world trees before restoration.
Any retained Generator IID6 holds the entire tree, including descendants, for
epoch-fenced reconciliation before initial generator spawning. Cleared ordinary
player drops remain restore candidates. Held trees retain their original binary
snapshots; malformed inputs return ownership rather than dropping records. `RegionService` connects this gate to epoch-fenced retirement before complete
region admission. Unknown legacy corpse identity remains held for reconciliation.

`magic_saves::freeze_magic_inventory` combines the exact bound component ticket,
reserved mana before/after values and pre-effect recovery checkpoint into one
placement transaction. Capture time is supplied for the preparation instant;
worker completion time must not replace it. The private pending receipt checks
all player/item acknowledgments, keeps uncertainty sticky, and never interprets
resource persistence as a successfully delivered spell effect. The simulation
holds mana against conflicting resource mutations while allowing health changes;
its exact receipt preserves those newer health values. These adapters still need
complete production gameplay/save qualification.

Portal freezes now join resource debits with the final destination or sanctuary
link in one exact placement operation. Player-death freezes join the final living
respawn checkpoint, purged/vitae registry, corpse and all inventory losses. This
is a durability hardening: a restart after commit restores the final alive state,
while an uninterrupted owner plays the source animation/portal delays. Frozen
operations must be retained rather than recalculated across uncertain outcomes.

Allegiance persistence has separate node/metadata CAS versions and a combined
placement transaction for corpse or NPC rewards. `PendingPlacementSave` and
`PendingAllegianceSave` retain the exact request and refuse to roll back after
an uncertain result; successful replies must match all expected acknowledgments.
Cold forest restoration uses one bounded PostgreSQL snapshot and validates the
indexed forest before owner admission. These services still require complete
production lifecycle qualification; they do not establish stock-client readiness.

`item_experience` prepares item XP/set assets from exact saved properties and
verified spell tables, then overlays frozen V3 item XP without losing Structure,
other properties or registries. `item_reward_join` checks exact inventory-ticket
correlation and joins byte-identical player snapshots across reward components;
a conflicting component leaves the complete base operation unchanged.
`restore_player_at_with_item_experience` requires cold metadata for equipped
levelable/set objects and transfers it with the drained player owner.

`StaffAccountService` routes the pinned account commands through the auth policy,
bounded password workers and PostgreSQL operation journal. It rechecks game
account authority, freezes a CAS request before submission, and retains that same
request across cancellation and uncertain outcomes. Host credentials remain a
separate trust boundary. Real PostgreSQL tests cover cancellation, retry and
access changes. `ChatApiService` supervises the bounded General/Trade/Audit HTTP
feed with separate scoped bot credentials. `GameRuntime` starts it after
authentication setup, passes its publisher to the accepted social output drain,
and shuts it down after that owner drains. The composed `serve` path still needs
full current-tree readiness validation.

`SocialService` drains accepted simulation social events through the canonical
player session projection counters. It retains one accepted event, recipient
progress and one complete network batch across backpressure. General, Trade and
Audit are published once into the supervised HTTP feed; private chat is excluded.
Admission-time friend events wait for the recipient's exact entered receipt;
waiting neither consumes the event nor advances its canonical counters.
Feed queue loss remains observable as a gap. Recovery returns the retained event,
remaining recipients and exact batch. `gameplay_dispatch` intercepts source Talk
`@` commands before chat; raw command Debug output is redacted.

Staff buff preparation uses the pinned Buffs declaration and SpellId enum, with
self/other fallback and missing-level-eight fallback. Golden C# vectors cover all
eight levels and both targets. Typed staff spellbook writes freeze a complete
player snapshot and wait for a correlated durable receipt; existing spellbook
probabilities and unrelated fields remain intact. Staff commands without an
implemented owner keep their authorized request and do not report success.

`region_service` drives bounded region preparation, PostgreSQL world-tree reads,
source equipment materialization, reserved GUID allocation, exact stale-generator
retirement, complete owner admission and durable unload checkpoints. One cold job
runs at a time on a separate bounded worker. Regional requests and outcomes retain
activation epochs; registry inputs and all saved items return intact on rejection.
The source cache is limited to 4,096 items and 64 MiB of encoded metadata. Its
configured region capacity must match the simulation residency limit. Generated
item services must register their exact transient source metadata with
`record_source`; absent metadata holds unload rather than guessing a template.
`poll` takes an explicit Unix/tick pair. Corpse V4 preserves the original source,
operation and absolute expiry; legacy missing identity holds restoration. Retry
keeps IDs, random identities, original clock conversion and frozen operation bytes.
`shutdown` returns the whole live service while preparation or saves remain pending.
The executable startup adapter must retain and poll this service alongside saves;
these bounded service tests do not constitute stock-client qualification.

`PlayerReadSnapshot` captures immutable committed state without logging out. The
routine freezer enforces identity and dirty revisions. Operation-scoped captures
require the exact pending namespace, operation and before revision; their baseline
freezer is only for composing the operation's atomic after-state, never a direct
routine write. Captures retain owned item revisions and registry supplements.

Complete region admission uses a separate bounded transfer lane because registry
owners are not cloned. Failed admission and blocked result delivery retain the
entire preparation. `RegionService` performs cold DAT/content/item restoration
and stale-generator retirement before admission; unload/expiry preserve corpse
schema 4 metadata. `game_clock` has an original C# day/night boundary oracle.
`game_bootstrap` verifies DAT fingerprints, opens the accepted immutable generation,
acquires the exclusive database world owner and restores the indexed allegiance
forest before gameplay admission. Production serving remains gated until the
remaining lifecycle, action routing and drain composition is complete.

Entry object projection now accepts authoritative position, motion, attachment and
sequence state explicitly. The cold appearance closure reads only fingerprint-
admitted DAT assets and is bounded by 4,096 source records/palette sets and 64 MiB.
Pinned ACE clothing layering, alternate setup lookup, saved helm/cloak options,
character hair metadata, body styles and palette fallback order are compared with
20 cases produced by the original C# methods (`oracle/entry_appearance.py`).
Eight additional original-method cases check object/physics optional-field flags
(`oracle/entry_object.py`); existing wire object fixtures cover their byte layouts.
House/hook object descriptions still require their separate live owner context.

`RewardService` retains one accepted character/item XP event and its exact ordered
network batches across observer-routing or network backpressure. Projection uses
`PlayerService`'s canonical session/property counters. Its recovery output includes
whether routing has already completed, preventing a shutdown retry from rerunning
sequence allocation. `SocialService` similarly publishes accepted General, Trade
and Audit messages to the scoped HTTP feed once, while retaining client output.

`OnlinePlayerSaveService` owns bounded admitted player/item baselines and five-second,
age-ordered captures through the simulation's immutable snapshot lane. Complete
owned item registries are frozen with their accepted aggregate revisions; unknown
properties and container placement remain preserved. Reserved routine capacity
writes one exact owned hierarchy batch. A valuable operation first waits for any
routine write or uncertain resolution, reserves all involved player inventories,
then hands back the exact newly committed snapshots, including acquired or retired
items. The controller does not treat a queued save as an acknowledgment. Dirty
notices arriving after a capture survive its receipt, and shutdown captures use a
fixed drain boundary. Correlation tokens come from the host's common snapshot
allocator. An unresolved request remains owned across cancellation; reconciliation
queries serialize behind the original PostgreSQL hierarchy writer before deciding
whether the complete batch committed. Caller composition must retain the service,
route its snapshot outcomes, poll its bounded write tickets, and block conflicting
critical work until this barrier admits it. These server-side mechanisms are not a
stock-client playability or outage loss-bound claim.
`PreparedPlayerEntry` composes the complete player description/title/friends,
self-object and possession plan with one canonical `EventSequencer`. It replaces
stale saved item relationships with accepted placements, requires every item
sequence owner, and orders object creates and container views like ACE's
`SendInventoryAndWieldedItems`. Native trees deeper than ACE's main-pack plus one
pack traversal are explicitly traversed with a 64-level bound, so no supported
nested item is silently omitted. Hand attachments are compared with 147 original
`GetPlacementLocation` vectors (`oracle/entry_attachments.py`).

`shard_control::ShardControl` owns the five pinned ACE shard commands with bounded,
sequenced replies, broadcasts and audit/log effects. Every command rechecks a
current staff principal. Its explicit monotonic clock fixes a pending shutdown's
deadline: changing the interval, including `stop-now` during a pending countdown,
does not move it. Countdown formatting intentionally omits the TimeSpan day
component. UTC timestamps and an explicit local-offset snapshot format the audit
messages without reading a wall clock. The original-source fixture is
`tests/fixtures/shard_commands.json`, regenerated by the admin C# oracle.

World-close boot requests select account access below Advocate and retain ACE's
boot text. Full shutdown progresses through player drain, session disconnect,
region unload and world stop. Each request remains owned until the exact receipt
reports no remaining entities, dirty saves or pending valuable operations. Effect
queue pressure rejects the whole command before mutation. Unlike ACE's destructive
five-minute failsafe, elapsed time never authorizes losing unsaved state; cancelling
a shutdown that has already entered draining is explicitly refused. New player
admission is blocked during draining. The application loop must deliver retained
effects and drive these real lifecycle owners before acknowledging completion;
this module does not itself force-stop threads or claim stock-client validation.

Player entry now has a separate trusted completion after the durable Online lease
and encoded entry batch are accepted. `PlayerService` retains that request and its
exact binding through simulation input/output pressure; UI requests remain refused
until the owner acknowledges entry. Duplicate entry calls cannot emit another login
sequence. The PostgreSQL lifecycle regression joins roster/creation, actual geometry
owner admission, Online fencing, ordered entry, a live UI mutation, owned routine
save, durable reload and ownership-preserving worker shutdown. The ignored
`player_assets` integration test separately prepares and admits an avatar using the
approved user-supplied DATs; neither test is a stock-client qualification claim.
The entry message order derives from pinned ACE
`WorldObjects/Player_Networking.cs::PlayerEnterWorld` and its `SendSelf` path;
`bace-replication/tests/session_output.rs` retains the independently specified wire
order/sequence regressions. The additional trusted completion/lease fence is a
BetterACE authority and durability boundary, not a new client opcode or a claim
about exact client portal-exit timing.

`SocialLookupService` resolves offline friend/squelch identities through one bounded
read worker and indexed PostgreSQL queries. It owns the exact authenticated action
until `SocialResolved` is admitted to the simulation queue. Ingress must retain
subsequent actions for that session behind this barrier. Read failures retain the
action for explicit retry; shutdown returns all unadmitted actions. Character and
account identities are checked again before the simulation caches them or updates
preferences. Saved-friend preparation uses a bounded ID batch; absent identities
remain explicit. No all-character scan or account-name disclosure is used.

Chat playtime uses an authoritative simulation tick anchor and frozen Int125.
Valuable holds defer revision changes while retaining elapsed playtime; release
accrues that elapsed time before a later routine/logout snapshot. Same-revision
age mutation and regression are rejected. Rejected/echo-only Turbine traffic is
projected privately and does not become an accepted General/Trade/Audit API event.

`generator_service::GeneratorService` owns bounded host-request composition: one
cold DAT/materialization worker, up to 64 immutable occurrences, 1,024 identities
per tree, and a 64 MiB materialized-input budget. It allocates dynamic IDs through
PostgreSQL, keeps the rolled result through slot refresh and queue pressure, and
requires exact owner receipts before releasing inputs. RegionService accepts each
complete immutable source batch before world admission so later acquisition and
unload retain the generated properties. Shop trees instead keep their descendant
sources in the vendor owner. A merged stock root discards the whole incoming tree;
its receipt names the retained stock identity. Correlations with high 16 bits
`0x4745` belong exclusively to this service and must be routed once by the host.

Generator lifecycle retirement consumes the owner's captured poses, inventory
revisions and enchantment registries, then freezes an epoch-fenced tombstone batch.
Save uncertainty retains the same operation, and owner completion follows the exact
durable receipt. Quiescing stops new ID seeding; shutdown cannot discard pending
materialization or live generator membership. Unused allocated IDs are returned
explicitly as allocator gaps after owner drain. The service's synthetic worker test
exercises actual simulation delivery, source-cache backpressure and bag/child
admission without claiming DAT or client qualification. Mixed creature/item treasure
batches use one source-ordered admission: placement failures omit only that root's
whole tree, while other failures roll back all freshly inserted actors. Vendor
actors use the same creature owner with source-defined passive combat behavior and
the vendor stock owner; authenticated first-Use lazy stock is connected while
live buying and selling remain separate interaction work.

The default Shop Buy adapter now has cold source-cloned fresh leaves, a retained
reservation/capture/save/confirm owner and a source-order private output batch.
Real PostgreSQL composite receipt, queue pressure and disconnect tests pass.
Authenticated ingress remains Unsupported until a joined end-to-end Buy test
proves the full route and baseline adoption.

Mixed, source-ordered Contain forests now retain ordinary Creature/Cow roots and
ordinary item trees atomically, including prepared delayed effects. Frozen item V4
companions preserve constructor origin, gear order and death-roster identities through
generic save paths. A dedicated durable constructed-creature promotion and cold
restore path retains exact Creature/Cow identities; recursive Contain source
selection now prepares nested Creature/Cow loadouts, equipment, spell assets and
companions under one reserved-ID forest. Focused synthetic runtime, simulation
and PostgreSQL checks pass. DAT-backed composed nested bind/admission/cold-reload,
specialized subtypes and nested generators remain gated. Authenticated live
Move of a constructed Creature/Cow uses the existing inspected world approach,
motion and range route, then checks a current open source-container generation.
Cold preparation requires each Creature/Cow in the inspected subtree to carry
its matching V4 construction companion. The exact promotion receipt preserves
the constructed owner, while ordinary inventory proposals still reject it.
The approved DAT fixture validates creature asset preparation only; a full
DAT-backed bind/save/player cold-login chain and acquired-owner logout/cold-login
handoff remain open. GamePiece requires the activity owner and remains
unsupported. ACE's factory maps obsolete AI type 16 to GenericObject,
not Creature. `oracle/factory/generate.py` compiles the unchanged factory switch
and monster-state method into 75 subtype and six combat-AI golden cases.

`player_entry::prepare_player_entry_state` joins the durable login receipt, the
canonical object counters, the simulation's accepted idle motion/body snapshot,
and verified setup metadata. Missing or conflicting inputs reject projection.
Player login uses ACE's initial pink-bubble physics defaults and actual nullable
property overrides; saved Int93 does not override that player-specific default.
The source's non-property physics getters are included in the calculation, as is
the post-construction IgnoreCollisions/ReportCollisions/Hidden override.
`oracle/entry_state.py` compiles the original methods and property wrappers into
528 vectors. The helper currently requires accepted grounded NonCombat/Ready
motion; moving-entry projection needs a complete accepted movement snapshot.

`CraftingService` holds a single authoritative tinker/salvage ticket, waits for any
preceding routine save, captures the exact reserved before-state, and retains one
frozen placement through uncertain database results. Uncertainty is sticky; a later
rejection cannot authorize rollback of an earlier possible commit. A publishable
completion requires both the exact simulation receipt and the online baseline
handoff. The service never rerolls crafting or generates replacement operation IDs.
Prepared salvage templates retain fresh identities, complete placement and all
source fields. Runtime tests cover real Kernel tinker/salvage ownership, unsaved UI,
item-only commits, full output queues, mismatched correlations and safe cancellation.
`finish_critical_for` names explicit reserved player roots for item-only transactions;
unchanged players are not rewritten and their dirty age remains intact. Source
formula/wire fixtures remain in the crafting, simulation and replication suites.
The standalone controller still needs the production ingress/output composition.

`GameRuntime` now owns the bounded authenticated lifecycle pump and its canonical
`PlayerService` counters. Fixed retained futures separate SQL, cold preparation and
world-service work from the event pump; the optional adapter loop polls at 5 ms
without creating per-player tasks. A degraded lane cannot starve unrelated save,
disconnect or output lanes. Region preparation errors, unmatched outcomes and cold
results retain their original owners. Unsupported ingress remains visible and
bounded; a terminal network event can still cancel unaccepted input and drain its
accepted character. The driver advertises Turbine chat only when the social owner
is composed. This remains a gated service composition, not stock-client qualification.

Login joins the actual verified DAT avatar/appearance closure, authenticated account
policy, durable `total_logins` receipt and accepted physics snapshot. Grounding may
recapture up to 300 authoritative snapshots; it never substitutes synthetic contact
or neutral movement. Saved `Character.IsPlussed` is a relational display flag read
under the Loading lease and does not grant account privileges. Logout first waits
for valuable work, then atomically detaches the complete player/item hierarchy and
captures its last live state. The detached owner and exact final batch survive
write uncertainty until every owned row and the Offline lease transition are
confirmed. The real PostgreSQL lifecycle regression covers a UI mutation after an
earlier routine save, final detached persistence, reconnect, and exact retry of the
Offline receipt. Additional tests cover missing descendant/stale detach rejection
and entry counter binding. Actual DAT preparation/admission/grounded entry is
covered separately; client testing was explicitly deferred.
Geometry-settled spawn poses mark a dirty character revision before admission,
so an accepted changed pose reaches the routine save. Admission-time friend
output waits for the exact entered receipt without consuming its event or
sequence. The approved-DAT synthetic fixture now covers real character
creation, authenticated UDP login, entered ownership, reliable entry output
and graceful drain; it does not represent stock-client playback.
An approved-DAT source probe also finds a registered authored lifestone in
the entered region. A geometry-valid authoritative ServerTeleport reaches a
near-stone accepted pose without moving the stone, and authenticated UDP Use
reaches binding cold preparation. The bounded player preparation worker reuses
its verified DAT archives for authored stance/action motion. An approved-DAT,
PostgreSQL synthetic authenticated Use now completes after revision retries;
source sound, motion and chat order, private stamina output, accepted final
actor and stationary stone views, and exact online/durable sanctuary and
stamina values are asserted. Stock-client playback remains unqualified.

Quiescing logs out admitted players, then sends correlated `QuiesceRegions` and
continues the ordinary region save/retirement pipeline. Pausing the adapter loop or
closing its control channel retains the entire live runtime; neither action is a
clean-shutdown receipt. The database world owner must survive until all remaining
simulation, ledger and output obligations have a complete durable drain proof.
On that proof, an idle quiesced world stays outside the worker queue. Failed NPC
placement clears only its exact prepared publication roots; accepted roots
retain their Spawned publication handoff.

The composed `GameRuntime` routes UI edits, XP expenditure and plain skill training
through one retained action per session. UI and XP changes enter ordinary dirty
captures; valuable training uses `SkillService` and emits its canonical-counter
success batch only after durable receipt adoption and online baseline handoff.
Pre-authorization holds retry the same intent; terminal domain failures do not.
Training operation entropy is allocated once on the adapter, never the simulation
thread. Disconnect/logout waits for the admitted action's resolution. Full skill
device routing remains separate integration work. Rank-up sound/chat and the
optional max-rank observer effect are connected. Ranked Endurance now appends
the full private Health update from accepted World current after those effects
in one retained ordered batch; a same-V6 regression checks the raised attribute
and Health state survive routine save/reload. Other derived vital/run-rate
effects and stock-client output remain unqualified.

The live staff lane first captures the current simulation-owned staff registration;
login-time role flags do not authorize account or shard operations. One retained
request covers cold map preparation, region admission, typed simulation execution,
and reliable output pressure. Map clicks use a single bounded DAT worker and the
accepted teleport epoch, never client height. Staff spellbook writes share routine
barriers, exact reserved snapshots, inventory companions and journal receipts.
The account lane retains its hashed operation through commit uncertainty and uses
ACE's five-second per-session password-command interval. Cold asset admission keeps
staff direct buffs in a separate effect index, including source aura/set flags;
it never installs gesture/component-free definitions into player casting assets.
Selection-dependent commands require an accepted selected-object owner. Arbitrary
staff casts prepare full native assets on a single bounded cold worker: verified
DAT spell/component/motion closure, account/foci formula, accepted server row and
projectile geometry. The exact character-bound program must be acknowledged by
the simulation owner before the retained cast command is submitted. Failed
admission recaptures current state; no account formula becomes a global fallback.
Normal-cast ingress must refresh these cached formulas when foci/augmentation
inputs change. Missing owner
families remain explicit retained work, not successful catalog entries. Observer
script/teleport batches retain bounded routing obligations until the accepted
visibility owner takes them; private output uses the canonical session counters.

Pinned `AdminCommands.HandleHeal` heals only players despite its catalog text
mentioning selected creatures. The live staff owner copies the selected object's
accepted display name before command admission, and the simulation emits the
source Broadcast rejection chat for nonplayers or missing landblock targets.
Absent visible source descriptions cannot supply a nonplayer name and produce
the source missing-target text. The simulation and wire regressions cover these
paths; the catalog remains metadata, not proof of every staff family.
The in-world ingress checks the concrete command owner list before reserving a
snapshot. Catalog-only and parsed-only text commands remain Unsupported; `sudo`
uses the same gate, and `@targetloc` accepts only its connected selected form.
Unavailable text commands get a bounded private unsupported response without
disconnecting the entered session.

`AllegianceService` joins exact indexed ledger before-images, all participant lease
fences (including offline ancestors), reserved player captures and item-XP/Vitae
companions. Database success is followed by the correlated simulation receipt
before the routine baseline advances. Uncertain operations retry identical bytes;
source NPC/corpse rewards remain owned by their complete composite transaction.
The live adapter supplies durable dynamic chat-room IDs and routes capture/control
results through the same bounded pumps. Real PostgreSQL tests cover owner adoption,
stale lease rollback and subsequent patch revisions; rejected and uncertain saves
retain their authoritative proposals.

The live allegiance adapter requests pinned-GDLE unclaimed-XP redemption once per
entered session generation. A prepared control outcome only holds the operation;
the binding is marked handled after the exact durable allegiance completion. Busy
controls retry, rejected writes retain the unclaimed pool for a later attempt, and
disconnect/re-entry uses a fresh generation. This connects the tested source
redemption proposal to the existing save and experience-output owner, without
claiming stock-client qualification.

The shard adapter composes `ShardControl` into authenticated staff execution,
closed-world admission and actual player/region drains. UTC-offset and initial
world-open policy are explicit startup inputs. Host audit/log obligations are
bounded and require an exact acknowledgment; final StopWorld remains held for
complete worker/database-world-owner recovery. Three live-adapter regressions
cover pressure, acknowledgment identity and the termination barrier. The network's
`TerminateAfterFlush` writes final queued datagrams before disconnecting, with a
five-second transport deadline; it never acknowledges remote delivery or saves.
Pinned AuthenticationHandler, Session.Terminate, PlayerManager.BootAllPlayers and
existing character-error/account-control wire fixtures establish the tested source
behavior. An eight-test loopback suite includes the final error bytes.

`VerifiedRegionAssets::prepare_material_names` reads the fingerprint-approved
material DualDidMapper on cold capacity. Synthetic original C# mapper and
RecipeManager.GetMaterialName fixtures cover record identity and display names.

The live inventory controller retains one bounded request through cold accepted
content preparation, exact simulation inspection, authoritative approach and
pickup animation, critical player capture, epoch-fenced atomic placement,
uncertain-commit resolution, owner receipt, and canonical replication. Inspection
and cancellation do not consume the action sequence. New split stacks use a fresh
accepted template and complete DAT-derived object description. World drops retain
the accepted swept pose and register their committed metadata with region
recovery; acquisition removes that metadata only after owner adoption. Output
pressure retains the exact batch and stops unattended approach movement.

Equipment preparation shares the ACE requirement/slot policies and login
physical/stat/magic profile builders. Noncombat equip/dequip holds the character,
complete gear roster, registries and vital pools through exact inventory durability,
then adopts candidate physical/magic/locomotion profiles, item XP, mana state and
vitals together. The visibility owner accepts the committed model/child graph
before the canonical private and retained observer batches are released. In-combat
equipment uses the authored stance transition under that same hold, and fresh
split-to-wield carries complete initialized stack state through the exact receipt.
Its committed output uses the fresh equipped identity and verifies the source
and created proposal rows before publication and visibility handoff.
This is server-side validation, not stock-client qualification; client testing
remains deferred.

The composed `GameRuntime` now routes authenticated tinkering `UseWithTarget` and
craft confirmations through bounded cold recipe preparation, a fresh accepted
player/item capture, the single simulation owner and `CraftingService`. The cold
catalog reads only cookbook/recipe namespaces with explicit row/byte limits and
pins the accepted pack generation. It joins actual native requirement/mod rows,
uses the source fallback selector, and admits only reviewed tinkering/imbue
recipes. Verified DAT material names and item appearance are loaded off the event
pump. A second capture after cold preparation avoids using stale actor revisions.

Confirmed and no-dialog operations retain one durable identity across pressure;
only exact committed owner results produce consumed-stack/remove/update packets.
Shared canonical counters project confirmation plus UseDone atomically. Decline
uses source YouChickenOut. Unrelated receipts are retained, never adopted. Local
broadcast obligations enter the existing bounded observer queue; missing observer
routing prevents a clean drain. Initial use now runs a fingerprint-admitted
ClapHands action through the existing authoritative motion owner before producing
the quote or drawing the craft result. It holds the character/registry inputs,
retains completion under output pressure, and never consumes a second action
sequence. Confirmation does not repeat the animation. Successful proficiency is
part of the same item/player transaction, including usage metadata, XP/skill
spend, level-up vitals and Vitae. Its canonical output preserves immediate skill
spend, optional no-dialog UseDone, then the queued XP grant. Original ACE numeric
fixtures, owner rollback/backpressure tests and a real DAT ClapHands preparation
regression cover these boundaries. Generic crafting and live salvage preparation
remain explicit integration gaps. Equipped and external-world targets are not
admitted by this owned-inventory route. Legacy invalid workmanship values require
a durable correction instead of a silent getter mutation. No stock-client testing
or complete crafting playability is claimed.


`PortalService` joins exact operation-scoped player captures, latest UI/registry
supplements and dirty owned items into one frozen placement operation. It holds
routine-save barriers through database uncertainty, correlated owner acceptance
and the actual matching portal effect. Independent effect/receipt channel order
cannot discard completion evidence. NPC/death composite transactions cannot use
this generic portal lane. `GameRuntime` now polls the lane and retains bounded,
sequence-acknowledged portal output obligations; packet/visibility handoff and
binding ingress still require composition. Retained output and
unmatched results prevent a clean drain. This does not enable client testing.

`equipment_effects` freezes source mana/affecting properties and exact player/item
registry companions under the same inventory revision and durable receipt. It
rejects stale before-images and missing changed-item participants before overlay.
`equipment_mana` prepares immutable known-spell targets from verified DAT metadata,
resets only ACE's transient accumulator/warning fields on reconstruction, and
prepares the documented exact-caster zero-mana restart cleanup. Routine inventory
snapshots overlay accepted Int107/Bool56 without resetting live transient work;
same-revision property differences are errors. Depletion notification uses one
bounded accepted social batch (Magic text then Sound.ItemManaDepleted), using the
existing session owner and preserving output pressure.

The live skill-device route handles authenticated inventory `Use` for source
SkillAlterationDevice (62) and the five skill AugmentationDevice (67) variants.
It captures current owned items, prepares all four authored wield requirements,
and supplies bounded revision-fenced profiles to the simulation before requesting
an official type-2/type-6 confirmation. The simulation resolves retired weapon
skills from current MoA skill projections again at confirmation, checks ownership
and costs, and reserves skill plus consumption as one operation. A separate
bounded `SkillService` instance retains the exact device proposal, scoped capture,
immutable durable write and receipt; it shares the same player save barriers and
canonical output counters as ordinary training. Success, stack/removal, refunds,
augmentation properties and scripts become visible only after exact adoption.

Source provenance is pinned ACE `SkillAlterationDevice.ActOnUse/VerifyRequirements/
CheckWieldRequirement/AlterSkill` and `AugmentationDevice.ActOnUse/DoAugmentation`.
Character transition oracles, original-source output traces, source-codec goldens,
owner revision/receipt tests, and retained adapter confirmation tests cover this
route. Missing/stale evidence fails closed. Non-skill augmentations and attribute
transfer devices are outside this adapter; they are not reinterpreted as skill
changes. Client testing remains deferred.

Recall startup resolves the eleven marketplace/PK/PKL destination classes through
the bounded accepted class-name index. Only absent classes or absent Destination
properties use ACE's pinned fallback positions; corrupt or mismatched accepted
records reject preparation. This does not load destination geometry or activate
disabled zones. Thirty-three unchanged C# static-initializer/Position vectors
cover exact float bits, null destinations and authored overrides. Cold recall
motion preparation shares the fingerprint-admitted avatar action closure with
crafting. Its source timer uses default-stance GetAnimationLength semantics,
including f32 accumulation, reverse rates and high-frame clipping; 288 original
C# timing vectors cover those rules. Marketplace retains its fixed 14-second
wait. Authenticated command recalls now retain one action per actor (64 maximum),
with one bounded cold preparation job, correlated geometry requests, revision
recapture before authorization, and atomic authored stance/action admission.
Movement remains admitted during the recall wait. The source timer begins with
admission, including when the stance transition is still playing.

`oracle/recall_output.py` executes unchanged source handler prefixes through
SendMotionAsCommands: 182 fixtures qualify admission notices, distinct mansion
errors, mana-before-mode-before-announcement order and their source IDs/text.
The live notice adapter uses canonical session/property counters and retained
observer routing. Infrastructure failures remain visible and retained; they are
not reported as source gameplay success. These fixtures exclude delayed teleport
continuations and network serialization. Portal packet/visibility composition
and an integrated announcement-versus-motion delivery trace remain unqualified;
constituent codec tests are not a claim of complete recall output parity.

Equipment visibility accepts a durable incarnation/revision/operation-scoped
model and attachment graph. It preserves accepted pose/motion and old immutable
blueprints held by reliable publications. Exact repeated handoffs are idempotent;
stale or conflicting handoffs remain with inventory. Subsequent observer views
carry new/updated children and retire removed child instances. This is server-side
output ownership, not stock-client qualification.

The skill-device adapter admits `ActivationResponse.Use` objects with generic
requirements and source `SharedCooldown`/`CooldownDuration`. Confirmed cooldown
rows join the skill/item transaction's frozen player aggregate, reload through
the enchantment registry, and enter the existing private cooldown projection
only after its exact receipt. Authored emotes, additional response flags, and
activation targets remain explicitly held. Initial Use emits UseDone after its
confirmation/error; confirmation callbacks and declines do not repeat UseDone,
matching `Player.TryUseItem` and `Confirmation_AlterSkill/Confirmation_Augmentation`.

Staff broadcasts retain the simulation-selected generation-fenced audience and
fan out bounded reliable messages without reprojecting sequence counters.
`configure_staff_broadcast_logging` defaults off, matching pinned
`chat_log_global=false`. When enabled, `peek_staff_broadcast_record` and exact
`acknowledge_staff_broadcast_record` expose a bounded host logging obligation;
shutdown remains blocked until it is accepted. This is AllBroadcast logging,
not an extra General/Trade/Audit feed event. Missing or replaced sessions cannot
receive stale audience messages.

The visibility delivery service joins cold complete public descriptions to
simulation-owned PVS/object-view snapshots. Player equipment exports only actual
public attachment children. Per-observer knowledge commits after every exact
reliable chunk receives admission; byte/count pressure, stale generations and
negative receipts preserve the prior knowledge state. Observer reset queues
remove-before-create work without starving behind unchanged PVS queries. Immutable
source revisions remain alive while pending publications reference them; the
service bounds live encoded source descriptions to 64 MiB, source versions to 65,536,
and retained reliable bytes separately. Actual equipment receipts replace the
public model/child graph, including explicit old-child removal/new-child creation.

Raw locomotion uses authenticated action sequencing, the current Body teleport
epoch, current admitted style assets, and live run/jump skill, strength, burden and
stamina. Jump preflights both the physical action and stamina mutation, so a replay,
airborne request or reserved vital cannot spend a prefix or reuse client velocity.
The worker recovery tests preserve every PVS/object-view correlation under output
pressure; Loading players stay invisible until their entered fence. These source
and boundary tests do not constitute a stock-client trajectory qualification.

Player death now has a retained trusted preparation/receipt lane and a live
`PlayerDeathService` checkpoint controller. It freezes the current operation-scoped
player and item state, corpse creation, inventory losses, registry purge/Vitae and
final respawn state in one world-epoch placement transaction. An uncertain result
retains identical operation bytes; neither SQL receipt nor animation alone releases
the player's routine-save barrier. The actual accepted respawn completes the
baseline handoff. Corpse expiry uses a separate retained world-epoch tombstone
operation. Transient descendants are frozen from their original retained source
archive, including construction metadata; a missing archive remains held.
Player-corpse expiry prepares actual DAT-backed world item bodies before saving:
direct contents move to the source spill poses while their descendant trees stay
intact. Monster contents become tombstones. The exact receipt publishes a
Destroying obligation and one simulation second later a Removed obligation;
immutable item snapshots and appearance assets accompany spilled roots.

`player_death_preparation` reads verified DAT model/motion/appearance and accepted
source templates off-thread. Original pinned ACE C# fixtures cover 12 corpse
appearance/identity cases and 20 final metadata/decay cases. Separate authored
geometry and save-worker tests cover ownership, output pressure, stale preparation,
uncertain retry and actual delayed respawn; those tests do not establish wire or
stock-client parity. LastOutsideDeath Position14 belongs to death metadata and is
saved in the same checkpoint. Int32 creation timestamp overflow is rejected.

The live death adapter retains immutable `DeathDelivery` obligations until each
supported canonical packet/visibility output enters its owner. Source death
announcement, accepted start motion, a bounded ordinary inventory-loss subset,
corpse visibility and portal-gated respawn vitals are connected.
ProtectionExpired and ProtectionDispelled now retain the simulation-frozen
entered generation and emit pinned queue-9 Magic system chat through the
bounded ordered network owner; pressure and same-generation disconnect retain
the obligation until admission or detach. Restored PK status 4/64 now projects
the source public PropertyInt 134 before private WeenieError with copied
counter and fanout preflight; other status values remain held. Fully consumed
non-container pack items with BondedStatus.Destroy now project the source
private remove and burden update before coin and selected-item effects; one
whole ordinary equipped corpse item with no spell or set effects now has exact
V5 receipt, private source dequip and observer appearance output. Other equipped,
container and coin destruction and dequip retain their obligations. Olthoi death selection
follows the source Slag/Treasure/Empty branches, preserves victim possessions,
and freezes the slag timestamp with the checkpoint. Original C# fixtures cover
100 slag/gland draws and timestamps; the source death-treasure engine supplies
tier-8 loot. Olthoi generated container descendants now retain source parent,
slot and GeneratorId through one corpse checkpoint, and the source Olthoi branch
has no victim-inventory loss output. Player Bool29 NoCorpse now has a separate
source-ordered cold selection, accepted world-root proposal and durable player
checkpoint/world-item placement operation. A committed zero-drop, fresh, or
direct-pack existing root can register world visibility after source-cache
admission. A selected direct-pack container retains nested contents in place,
touches their revisions and freezes V5 snapshots with exact parent and slot
checks in the same
placement receipt. Its Completed branch publishes the single-owner burden
correction. Selected equipped roots remain retained at Completed pending the
source dequip, appearance and observer transcript. Ordinary world-container
Open remains a separate unsupported Use route. Pinned
`Creature_Death.GenerateTreasure` selects both
fresh treasure and preexisting wielded/inventory objects by authored
`DestinationType`. `ItemSaveV5` now preserves a known source origin through
ordinary inventory saves, while migrated legacy rows remain unknown. The
player death NoCorpse selector uses the known origin and rejects legacy rows
whose origin cannot be proven. Its receipt freezes eligibility, selected
create-list results, every existing and fresh root, copied death pose and quest
GeneratorId. Full Bool29 output remains unsupported for selected equipped
inventory. Pinned ACE copies `Location` into each root; BetterACE copies the
accepted world-owner pose and rejects any geometry result that moves it. This
is an authority correction, not a universal source parity claim. Cold corpse preparation
waits for accepted grounded, stationary physics instead of using a synthetic or
client pose; this may delay a falling death beyond source presentation timing.
The final-item inventory transaction shortens a corpse's persisted deadline to
15 seconds without extending an earlier expiry. Twelve original C# empty-decay
branch cases and 24 original spill-position/resting-placement/delay statement
cases supplement the owner and freezer tests. Authenticated corpse Use now
checks authoritative range and access, freezes Open/Close state and defers expiry
for an active viewer. Open projects direct contents and one nested level under
each subcontainer in pinned ACE's ViewContents and CreateObject order; deeper
content requires a later container Use. Repeated Open resends CreateObject for
known direct/nested children with their canonical sequence owners, matching
the pinned source loop. Permission-origin GameActions and their
durable updates now route through a simulation-owned recipient grant table with one-hour expiry,
offline list/removal by the frozen granter name, the source consent option, and
logout cleanup. A successful durable Open consumes the transient grant and
retains the one-corpse permittee in the versioned corpse profile. Other source
rights/output paths remain held where exact owner evidence is unavailable.
Pinned ACE keeps that per-corpse permittee set ephemeral; persisting confirmed
rights across restart is a deliberate durability correction.

Staff gag commands use one retained controller operation. Indexed identity lookup
precedes simulation authorization; online targets then share the routine-save
critical barrier and an exact `StaffGag` snapshot capture. Offline targets use an
offline lease checked again in the database transaction. A completed load remains
owned across decoding failures, and uncertain writes retry the same journal ID,
lease and bytes. Audit and issuer success output follow durable owner adoption.
The source gag duration is online heartbeat time, not offline elapsed wall time;
reconstruction resets only the source transient notice flag. The source properties
remain in the existing frozen player DTO, without introducing another save version.

Exact broadcast logging uses the existing bounded diagnostic worker. Queue pressure
retains the source record; closed and oversized handoffs report an explicit failure.
Exact records have a 32 KiB encoded bound so escaping a maximum accepted chat input
cannot silently truncate it. A successful handoff acknowledges the worker queue,
not disk durability; disk health remains visible separately.


Final corpse-child pickup retains the complete CorpseSaveV4 baseline beside cold
item metadata. Its revision-only corpse update and shortened absolute deadline
share the valuable placement transaction, preserving death identity and appearance.
The deadline is frozen once from explicit captured tick/UTC, never recomputed by a
retry. Ordinary generic inventory freezing remains item-shaped for existing death
and region adapters; InventoryService performs the exact corpse wrapper overlay.

Portal private batches now retain their exact bytes until a generation-fenced
reliable-peer admission receipt. Generic visibility waits for that receipt and
the matching observer work. Before canonical portal projection, older prepared
visibility packets drain; pending read-only queries can be cancelled without
advancing knowledge or packet counters. The bounded observer router snapshots
only the established knowledge audience, retains exact fanout chunks under queue
pressure, and handles each generation's reliable receipt before advancing.
Teleport resets use an event identity independently of simulation time, including
two same-tick resets. This is output ownership and ordering hardening; reliable
admission is not remote delivery or a new claim of stock-client parity.

Initial login materialization waits for the accepted authenticated LoginComplete
receipt before its canonical private/observer batch. The component override
(Bool 68 false when components are disabled) is retained in that reliable batch
once per entered character binding, matching the pinned ACE
`FirstEnterWorldDone` one-shot rule. Readiness and one-shot markers are fenced
to that binding and cleared when it leaves the authenticated session.
ACE sends the override in its LoginComplete handler even if landblock completion
later delays materialization; BetterACE deliberately keeps it with the accepted
materialization publication so the readiness fence cannot lose the override.
Backend tests cover an event preceding its control receipt and a duplicate
initial materialization. No stock-client ordering claim follows from those tests.

Disconnected portal completion transfers to the existing trusted detach owner
only after its delivery obligations drain. Exact failed detach receipts retain
the transit; success removes it after the kernel's portal-link lifecycle cleanup.
No client-ready or materialization event is fabricated. Definite stale readiness
and recall authorization rejections remain per-session diagnostics and cannot
hold the global portal/recall result lane.

Recall and LoginComplete adapters use the accepted reliable-message sequence for
the existing simulation authorization fence. Their independent client GameAction
counter is decoded but cannot rewind or exhaust that fence. Mixed-traffic tests
cover small, repeated and maximum client counters while preserving the exact
server-bound actor/account/session. This is authority hardening, not a change to
the wire envelope layout.

`oracle/binding_output.py` executes unchanged Lifestone/Bindstone ActOnUse bodies
for 480 cases. `tests/binding_source.rs` compares the implemented eligibility,
final-radius, position-copy and float stamina-rounding subset. Fixed action-chain
and motion adapters expose source call ordering but do not qualify the live Use
pipeline or authored timing. The new NoCorpse and constructed-restore oracles
provide 108 and 256 source cases respectively; their helper exclusions are recorded
with the generators. Those two oracle sets prepare the remaining implementation
work and are not claims that NoCorpse or creature restoration is enabled.

The recall-output, portal-state, binding-output and NoCorpse oracle scripts use
.NET 8 from `PATH`, or the executable specified by `BACE_DOTNET`. They run only
against the pinned local ACE reference and do not require a game client.

Projectile publication retains the accepted launch pose, velocity and exact
entered-session/teleport PVS audience on the simulation owner. Cold appearance
preparation or reliable-output pressure can therefore outlive a short flight
without dropping its Create. The visibility service stages one-object knowledge
through the existing receipt path, preserves unrelated known objects, and fences
reentry/teleport. It uses source 112.5m initial visibility and the existing object
counter owner. Launch observers come from indexed neighbouring cells, including
reverse indoor VisibleCells membership; no client pose or placeholder model is
used. Owner and runtime regressions cover disappearance before publication,
pressure, exact audience and ordered retirement. Stock-client timing remains an
end-to-end acceptance requirement.

Shop creature Use prepares source-ordered `DestinationType.Shop` stock from
the admitted vendor's pinned generation. A durable V5 world vendor, immutable
authored source hash, exact marker and contained-item transaction, simulation
reservation, and confirmed private `ApproachVendor` publication gate first Use.
The marker and item forest can be adopted after restart or viewed again without
rerolling identities. Empty authored stock still commits a loaded marker. Buy,
Sell, alternate currency, transient vendor promotion and later stock revisions
remain unsupported as live actions. Default Buy now has an exact freezer for
player V6 CoinValue and burden, selected coin V5 debits, fresh source-cloned
grant V5 rows, vendor Int77/79 counters and marker revision. Vendor stock writes
enter the bounded critical save lane; a full lane retains the exact operation.
The canonical inventory sequencer can order a private pickup sound, refreshed
ApproachVendor and UseDone. Buy ingress still needs fresh grant asset preparation,
joined snapshot and critical lease, exact submission and baseline adoption, and
one retained committed output batch before it can report success.
The focused runtime fixture lacks DAT geometry, so its preparation and output
tests do not establish a complete authenticated live Use transcript.

Skill-device `ActivationResponse.Use|Talk` (Int83 `0x12`) prepares bounded
`ActivationTalk` String17 and emits the pinned Broadcast system chat after the
confirmation response and before UseDone in one ordered batch. The generic
activation requirement owner still checks Use and confirmation. Other response
flags, activation targets and emote effects remain held. The Talk fixture uses
pinned ACE `WorldObject_Use.OnActivate` and `OnTalk` order; it does not claim
complete activation parity.

For Int119 Active=0 on a skill-device object, authenticated Use captures the
accepted item revision and asks the simulation owner to authorize an inactive
action. The source's OnActivate returns before requirements, cooldown, emotes
and ActOnUse; only the outer Player_Use UseDone is projected. Replayed or stale
item actions receive a generic cannot-use UseDone without a quote, skill change
or item consumption. This stale-input error is an authority hardening path.
AttributeTransferDevice Int119 Active=0 follows the same authenticated
accepted-revision inactive Use rule and projects only UseDone. Stale or replayed
actions return cannot-use without a quote or consumption. Generic activation
effects for active attribute devices remain held.
