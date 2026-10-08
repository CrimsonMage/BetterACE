# Implementation status

This file records verified capability, not the architecture's intended outcome.

| Milestone | Status |
|---|---|
| A: workspace and governance | Implemented: inventoried workspace, thin roots, dependency/size checks and CI |
| B: protocol, assets, storage and content foundations | In progress |
| C: two stock clients in an authoritative world | Not implemented |
| D: full failure/performance acceptance | Not verified |

The crate inventory uses `scaffolded`, `foundation`, and `implemented` to distinguish declared ownership, partial working foundations, and a completed stated crate scope. Protocol compatibility is assessed separately; no whole-server parity is claimed.

## Active implementation and validation — 2026-10-08

Client testing is explicitly deferred by the project owner. Server-side unit,
source-oracle, database, asset and architecture checks continue; no client
playability claim follows from those checks.
Initial-login portal materialization now waits for its exact accepted
ClientReady receipt. Readiness and the one-time Bool68 override are tied to the
entered character binding and cleared when that binding leaves the connection.
All 15 focused portal tests and strict targeted Clippy pass. BetterACE keeps the
override in the reliable materialization batch; pinned ACE can send it before a
landblock-delayed materialization, as recorded in the divergence register.

Current integration work adds immutable online player captures, operation-scoped
committed-before captures, and retained atomic player/region admission through the
simulation worker. Full output queues preserve rejected prepared owners and keep
physics ticking. Regional cold restoration now carries saved item forests,
registries and corpse deadlines into the same admission before generators run.

Corpse schema 4 stores the original source actor and simulation death operation.
Legacy migration preserves unknown identity instead of parsing opaque journal
keys. Expiry/unload retain the wrapper; PostgreSQL rejects changes to a persisted
corpse's death identity or deadline. Three schema tests and the real PostgreSQL
corpse transfer/migration test pass at this checkpoint.
The V5 access transition now admits the one-time V3/V4 migration that fills an
unknown source identity at a newer revision; later identity changes remain
rejected. The V5 codec regression and real PostgreSQL V3-to-V4 transfer test
pass. This repairs a full-workspace test failure found during integration.

The generator clock has 391 original ACE C# boundary vectors. Its constant-time
hour calculation preserves quarter-hour rounding. The source's maximum-time
expression is 1,073,741,824 seconds; its comment incorrectly says 1,073,741,828.
Two runtime clock tests pass. Player entry appearance, attachments, inventory order
and property visibility have original C# fixtures. The entry inventory discriminator
now preserves ACE's backpack-slot check and Container type 21. Full live routing
and service composition remain under active implementation.

Generated item forests now admit successful world roots with their complete
contents, discard failed root trees, and reject malformed parents before mutation.
Request-specific IDs and correlated placement refreshes preserve the original RNG
intent without requeueing behind unrelated requests. Retirement captures include
accepted physical poses and current enchantment rows for surviving slot-compacted
siblings. Twenty focused generator inventory tests pass; the original ACE
per-object Spawn loop generated 40 reference cases. Vendor stock now retains container descendants under the one vendor owner;
merges preserve the original contents and release the incoming tree. The original
ACE AddDefaultItem method supplies independent retained-tree fixtures. Mixed
creature/item treasure admission passes four focused tests. Vendor preparation
uses real creature assets and the source passive-NPC predicate; GamePiece remains
explicitly unsupported. Both actual-DAT generator admission tests pass.

The skill operation service retains one exact freeze through uncertain commits,
waits for routine-save barriers, and updates online baselines only after simulation
adopts the committed receipt. Its owner regression includes an unsaved UI change
alongside skill training. Actual DAT cold avatar preparation/admission has passed
without disabling geometry checks. These are focused checkpoints, not a final
workspace validation result.

The crafting operation service passes four connected owner tests: complete tinker
and salvage saves, unchanged-player item-only commits, queue-pressure retention,
exact retries through uncertain database outcomes, and cancellation before writes.
Captures preserve pending UI changes and item registries. Dirty item-only operations
release explicit player reservations without inventing player revisions or rewriting
unchanged player rows. Production crafting ingress/output composition is still work.
The cold avatar worker also passes actual-DAT preparation through accepted grounded
entry projection, including the wrapped durable login instance. This runs the
server's simulation and asset readers, not a client.

Live runtime ingress now routes authenticated UI edits, XP expenditure and skill
training through bounded per-session barriers. Training retains its durable random
operation identity across retry and reports success only after owner adoption and
baseline handoff. Primary progression output borrows the existing session counters.
Four controller regressions and six progression projection tests pass; 440 notices
match the original ACE C# training handler and skill names. Rank-up effects and skill
device routing remain separate integration work. A reserved XP action now returns
an explicit pre-authorization retry outcome instead of blocking a later inventory
commit in the simulation queue; its same-sequence retry regression passes.

Shard commands now connect to the live adapter with source closed-world admission,
privileged admission while merely closed, bounded retained broadcast/host-audit
outputs, and actual player/region drain checks. Three adapter regressions pass.
Terminal network messages flush to the UDP socket before disconnect; the eight
loopback tests pass. This is transmission, not a remote delivery or save receipt.
Host log acknowledgments and final world-owner release remain explicit lifecycle
obligations; shard shutdown does not manufacture a completed-world proof.

Crafting spellbook freezes preserve existing probabilities and use ACE's 2.0
default only for newly added spells. Three frozen-save regressions pass, including
native script tinker-count reconciliation. Five original C# GetOrAddKnownSpell
vectors establish the existing/new-entry behavior. The active mutation scripts,
live crafting controller and full result projection remain under integration.

Portal durability now has four connected service regressions: exact retry bytes,
pre-write cancellation, duplicate-participant rejection and effect-before-receipt
channel ordering. The extended-save simulation test verifies both successful and
rejected recalls permit casting again and leave the actual stationary lifestone
and saved link unchanged. The live adapter retains bounded portal output with
exact acknowledgments; complete ingress and canonical visibility/packet handoff
remain unfinished. All seven recall input handlers have fourteen original C#
vectors and separate world-session/budget tests.

A new relational login counter increments atomically with the exact Loading→Online
lease transition. Two actual PostgreSQL regressions cover duplicate completion,
abort/reconnect, restart and exhaustion. Seven original ACE login/ushort-sequence
vectors verify the projection; exhausted signed counters reject admission before
state changes. Historical native characters begin a new counter history at migration
because earlier native saves did not retain ACE TotalLogins.

`serve` now starts the composed game runtime, while live routing, critical
save/lifecycle composition and shutdown ownership continue to be qualified.
Full workspace validation is also not yet
complete: concurrent combat/NPC changes have produced transient compile failures.
This paragraph supersedes older test-count milestones below, which remain
historical records rather than current full-tree results.

## Working foundations

| Area | Implemented evidence | Remaining boundary |
|---|---|---|
| Protocol | Pinned official C# golden vectors; full named opcode catalogs, character/property/DDD/movement/object/social codecs, bounded peer driver and paired UDP worker | Complete production lifecycle composition, remaining payload/action families, full replication and stock-client transcript qualification |
| Accounts | Bounded Argon2id workers; canonical PostgreSQL accounts; race-safe optional auto-creation (default on); durable account-ban verdict/write/expiry and post-cookie ban response; generation-fenced account/session drain | Complete stock-client character flow, issuer-detach Audit recovery and production service composition |
| Assets | Bounded archive/outdoor readers plus full XPTable, SkillTable and CharGen readers; real portal tables prepared for 38 skills and 13 heritages | Motion/setup/animation/cell/BSP/environment decoders now have oracle and real-asset evidence; movement response, collision admission and portal traversal remain uncomposed |
| Physics/world | Single-owner synthetic motion/PvE/door orchestration plus source-backed GDLE BSP placement/contact queries | Authentic AC motion/physics and stock-client reconciliation; no SIMD conversion is qualified yet |
| Runtime | Dedicated simulation owner; bounded command/result delivery and kernel recovery; tested UDP-to-progression-to-primary-reply path; separate content/save adapters | Composed gameplay lifecycle, accepted/rejected action routing, projections, asset loading and save integration |
| Content | Typed full weenie property families; native TOML/binary round trips; immutable revisioned catalogs and indexes | Runtime instantiation/spawning and system-specific interpretation |
| Content Studio | Polished egui weenie editor, Rust EmoteScript, native loot profiles, DAT static model/palette/ClothingBase inspector, authoring helpers, safe TOML saves, legacy conversion and incremental mapped-pack builds | Full EmoteScript/client renderer parity, runtime loot/pack publication and non-Linux SQL staging; Windows/macOS validation outstanding |
| Imports | Checked ACE Entity/Lifestoned JSON; isolated MariaDB execution of unchanged weenie SQL, all 24 weenie/property tables, count/hash manifest | Complete pinned release now converts without table loss; runtime interpretation/admission of all systems remains unfinished |
| Storage | Binary aggregate PostgreSQL schema, migrations, revision CAS, atomic/idempotent operations, durable candidate journal and quarantine | Frozen player/item/corpse/housing schemas, identity/ownership constraints and fenced transactions now exist; full gameplay mutation integration remains unfinished |
| Live content | Direct candidate insert and reviewed host inbox share a trigger journal; mixed typed world/weenie/profile/ClothingBase deltas, explicit tombstones, bounded validation, durable accepted heads and atomic mapped generation delivery. Accepted delta keys schedule affected occupied regions; owner-fenced plain static add/remove and visibility retirement preserve old actors and durable trees. Same-profile generators can issue future intents at the new revision. | Complex NPC/generator root migration, structural generator-profile edits, existing-actor replacement and ClothingBase appearance refresh remain held. Approved-DAT occupied-client add/remove replay is not yet qualified; pack acceptance alone is not gameplay. |
| Save fairness | Dedicated OS thread/current-thread runtime, reserved writer connection, dirty-age preservation, bounded critical bursts and alternating routine/offline queues | Composition with gameplay ownership and logout; production health export/load qualification |
| Mapped packs | Streaming immutable `.bace` compiler, bounded authenticated indexes, lazy payload integrity checks, base/delta lookup, tombstones, restartable manifests; PostgreSQL accepted-generation transaction | Complete-world aggregate compiler, bounded pack-I/O/region preparation and compaction now exist; world adoption, runtime publication and reader-safe reclamation remain uncomposed |
| Offline ownership | PostgreSQL epoch fencing, login/logout CAS and idempotent offline cached-XP receipts | Full allegiance calculation, online XP routing, restart lease takeover and owned multi-character transactions |
| Host console | Authenticated loopback dashboard, reviewed TOML inbox with exact add/replace/remove preview and confirmation, bounded logs and reusable supervisor/child drain protocol | Gameplay drain composition; Windows private-ACL provisioning and platform qualification |
| Clothing | Native ClothingBase editor, bounded legacy mod JSON import/export, multi-part/texture/palette merges, supplied-DAT previews and a versioned pack record accepted through reviewed inbox | Runtime equipped-layer composition, game-world override lookup and observer appearance refresh remain unintegrated |

`bace-server serve` starts the composed game child when its accepted pack, database and approved assets are available. `check`, `exercise`, and `content-worker` remain usable tools. The host console reports the child’s actual preparation, running and drain states. Server-side admission and gameplay paths are connected but do not establish stock-client playability or complete parity. See `host-console.md`, `pack-format.md` and `persistence.md` for foundation boundaries.

## Next implementation sequence

1. Port and oracle-test AC motion tables, setup/cell/BSP/portal assets and authoritative movement. Retain scalar reference arithmetic; qualify SIMD only after profiling and the gates in `physics-math.md`.
2. Complete reliable UDP/session orchestration and stock-client login, character creation/selection and world-entry messages, with pinned transcript fixtures.
3. Implement character/spawning/interaction/replication slices and integrate catalog generation swaps on the simulation owner. Add frozen character save DTOs, dirty tracking, valuable-operation reservation and bounded shutdown drain.
4. Convert the remaining official-world systems before accepting the complete pinned dump. Run two actual stock clients through outdoors, dungeon, jump, portal, nearby-player visibility, reconnect and live new-weenie spawn scenarios.
5. Run paced ten-minute mixed-load and failure acceptance, including delayed/failed saves, packet loss/reordering, malformed packets, forged movement and late content. Compare measured memory, CPU, storage size and query/save costs with equivalent ACE workloads.

These remain implementation work, not merely missing test runs. Current synthetic benchmarks do not establish these milestones. See `validation.md` for the checks actually performed.


## Networking expansion evidence

`docs/network-coverage.toml` inventories immutable official network sources and
compiled codec dependencies with SHA-256/Git-blob provenance, ownership and
explicit unsupported/foundation statuses. `cargo xtask check` checks its structure;
`tools/bace-compat/oracle/network_inventory.py --check --source <pin-directory>`
checks coverage against the official Git tree. Identifier coverage is separate
from payload, dispatch, gameplay and stock-client compatibility.

The independent C# oracles now cover named catalogs, selected messages, movement
conditional layouts and queue/retransmit behavior. Character and quest harnesses
exercise thousands of synthetic scenarios using verbatim pinned methods. Runtime
loopback tests cover handshake recovery, pre-cookie rejection, reliable delivery,
duplicate-account drain, bounded authentication and DAT-worker result retention.
None of these tests establishes a playable shard.

The requested full networking/full gameplay scope is **not complete**. Remaining
work includes remaining appraisal/combat/magic payload families and complete gameplay-backed object/social handling,
remaining action handlers, production character/save schemas, authoritative AC
asset/world composition, complete replication and all gameplay-owner milestones
in `docs/gameplay-parity.toml`. `serve` runs the composed server-side path without
a stock-client qualification claim. The supplied divergence
register is tracked in `docs/divergences.toml`; reports from another ACE revision
are leads to validate, not automatic changes to BACE's pinned authority.


## BetterACE continuation

The repository/project display name is now **BetterACE**. The `bace-` package and
executable prefixes, `BACE_*` environment variables and `.bace` format remain stable.

Full object creation/update/appearance/physics/game-data codecs and Turbine/social
codecs now have pinned C# vectors. XP, skill and character-generation DAT tables
have independent decoders and actual-asset validation. Character training and
specialization preserve authoritative metadata and reject unsupported augmentations.
The primary progression request/result path is connected in a real UDP integration
test with a synthetic world. It does not establish stock-client world readiness or
full rank-up side effects, persistence, geometry, object visibility or gameplay parity.
Accepted rank changes now freeze an authoritative base/maximum view in simulation;
the private RaiseTrait sound, Advancement chat and max-rank WeddingBliss observer
effect have a source-derived bounded projector. Math-only character fixtures lack
the prepared inputs for those effects, and the live path retains such a missing
rank view as an output error. Derived vital follow-up, optional run-rate hooks and
complete stock-client transcript qualification remain open.
Skill alteration and augmentation Use now carry source generic activation
requirements into the simulation for checking on both the initial quote and
confirmed operation. Authored SharedCooldown/positive CooldownDuration now use
an exact registry before/after receipt in the same durable skill/item operation;
confirmed completion projects the private cooldown enchantment. Activation
targets, emotes and additional responses still require an owning transaction
and remain gated.


Inventory/trade/vendor wire requests and event/listing payloads now have explicit
bounded codecs and pinned fixtures. They do not implement item ownership, trade
transactions, vendor pricing or durable success policy. Turbine framing preserves
the pinned +8 length arithmetic and ByName response dispatch; oversized lossy
UTF-16 prefixes are rejected with documented hardening tests.


## Whole-world PvE expansion — implementation checkpoint

The requested complete playable milestone is **not yet achieved**. `serve` now
opens the accepted world pack and verified assets, starts the game runtime and
reports actual service state. The separate `check` command reports remaining
stock-client readiness evidence.

- **World on disk:** the complete pinned release converts to one aggregate base
  containing 43,913 weenies, 915,050 typed source rows, 38,152 regional indexes and
  14,862 parent-instance link indexes plus native table records: 1,011,980 records
  in the current private compiler output. PostgreSQL contains only
  accepted pack manifest/publication metadata, not a bulk copy of these world rows.
  One base plus at most two active deltas is enforced by codec and runtime APIs.
- **Preparation:** a separate bounded worker can load Holtburg's 25 authored
  instances and one encounter from mapped regional indexes, sharing templates.
  Required missing templates reject the preparation atomically. Data preparation
  does not grant collision admission or create an authoritative actor.
- **Persistence:** player/name/account identity, starting gear, relational item
  locations, housing ownership and frozen player/item/corpse/house schemas exist.
  Offline writes share login fences. Inventory/house operations have stable durable
  receipts and CAS; character creation records a durable receipt too. World-owner
  locking and explicit restart fencing preserve the last durable state.
- **Network/lifecycle:** source-backed PlayerDescription, combat and character
  lifecycle packets; ordered per-queue login projection; mixed-queue atomic network
  admission; authenticated roster/prepared creation/load/admit/abort/logout APIs;
  bounded combat outputs and shutdown recovery. These are integrated test paths,
  not a production session loop or a claim that the stock client entered the world.
- **Gameplay:** source-backed skill checks, contributor reward calculations,
  create-list selection and vendor pricing; bounded NPC script/spawn/transfer
  foundations; synthetic-authority melee with timed hooks and damage/death outcomes;
  a humanoid character factory with explicit prepared assets. Full physical damage,
  native NPC scripts/services, generated death treasure and real-world composition
  must still be distinguished from these foundations.

Remaining playable gates include authentic AC movement/collision and reconciliation,
wire/DAT/content creation preparation, full entity/possession projections, runtime
session orchestration, autonomous world population, complete service NPC and required
magic behavior, and production gameplay-backed save/drain/hot-publication composition. The
local world artifacts and database do not make those missing implementations pass.
The divergence register remains active, especially melee #2–10/#37/#50,
interaction/appraisal #17–19/#28/#41–44, Town Crier #57, lifestone aliasing and the
teleport visibility race. Preserve source uncertainty when reviewing these cases.


The GDLE authority extension now includes server-stamped door animation requests,
separate physical solidity, occupied-close retries and NPC collision-triggered use;
client contact/open-state claims do not control those transitions. Monster pursuit
uses authoritative target/home/chase checks. These are connected synthetic-world
regressions. The scalar GDLE BSP slice separately validates placement/contact and
radius-subdivision observations against compiled source and actual dungeon inputs;
it is not a complete slide/step/portal solver or accepted-position producer.

Bounded frozen possession loading and routine owned-aggregate batches now protect
online/offline players, items and housing without rewriting clean ancestor snapshots.
They share the save worker's reserved age/deadline service. All registered player and
house writes also check embedded identity against relational ownership before any
batch mutation. World activation validates complete derived-index membership so a
reindex cannot silently hide source objects.

## Native loot, isolated rares and durability expansion (2026-10-07)

The native gameplay data path now includes bounded nested loot tables, item
mutation subtables, explicit rare profiles, isolated character rare streams and
frozen death proposals. `native-loot-and-rares.md` documents authoring, evidence
policy and operation boundaries. Ordinary loot uses event-local randomness;
character rare ordinals/timers persist with player saves. No uncertain retail
rare probability is enabled by default.

`freeze_native_death` builds an atomic corpse/item/player placement operation.
Real PostgreSQL tests cover stale-CAS rollback, exact-request replay, rare-state
advance, generated mutations and preservation of later save supplements. Native
profile publication uses the same durable journal for tooling and SQL inserts,
checks referenced templates, preserves rejected heads, and compacts before a
fourth active pack. Mixed weenie/profile transactions and reviewed world-row
changes publish as one accepted generation or are rejected together.

Simulation-worker channels now retain NPC proposals/notifications, cast results,
magic effects, inventory and housing proposals across bounded-output pressure and
shutdown recovery. This is transport between owners, not a durability receipt.
New subsystem implementations and source fixtures do not establish stock-client
readiness. Authentic world admission/movement, complete stock-client
service qualification, all magic families and all NPC service effects remain
unfinished; only tested, connected owner effects count as implemented.


Character features (2026-10-07): skill train/spec/lower/reset and supported skill
augmentations use immutable proposals and exact durable receipts. All four equipped
wield requirements are rechecked before lowering. DAT taboo preparation and opaque
name approvals reject lowercase initial characters, normalize all-uppercase single
words, and enforce prepared creature/taboo exclusions. Per-character UI includes
both masks, shortcuts, eight spell bars, filters, desired components and the opaque
layout blob. Frozen schema3 and explicit migrations retain player/item/corpse/house
enchantment entries; active timers dirty the owning aggregate and offline transfer
suspends them. Real PostgreSQL tests cover retry, fences and downgrade rejection.

Tinkering uses source-derived chances with owner-directed exact 33%/38% imbue caps,
independent character/operation streams and skill scaling. GDLE salvage rejects
stacks intact and preserves every output bag and unsuitable input ID. Numerical
oracles and a predeclared per-player 1.28-million-trial statistical regression cover
these policies. Unsupported recipe scripts/result-creation families still reject
preparation. These are implemented subsystem and adapter paths, not a claim that
stock clients can complete world entry or that every ACE gameplay family is complete.

| Gameplay addition | Tested implementation | Explicit remaining work |
|---|---|---|
| Magic | GDLE cast-control vectors; per-actor saved recovery; Life-projectile source drains and launch-time lead; direct vitals, projectiles/rings, registry/periodic and server-origin effects; World-owned motion completion in connected chain tests | Actual motion-table factory and stock-client qualification, stalled-server clock parity, full gear/critical/resistance and enchantment stat propagation, target masks, arc/strike trajectories and complete service composition remain unfinished |
| Native NPC emotes | ACE timeline/branch scheduling fixtures; property/quest/XP/luminance and character-service proposals; atomic PostgreSQL effect/checkpoint stages; exact receipt and recovery-readiness gates | Every named owner service, full sharing/allegiance/item-XP/vitae stages, hand-in routing and production continuation recovery remain incomplete; queued XP is being connected at its source-defined admission and recipient stages |
| Inventory and housing | Placement proposals, container/equipment/stack rules, atomic PostgreSQL transfers, retained-content eviction, stale-access fences, and latest-schema-preserving freeze/load adapters | Static house/hook/storage associations, allegiance-derived access, prepared currency change and stock-client packet/physical-world qualification remain incomplete |
| Rares and nested loot | Native editable graph/mutations, isolated persistent character streams, top-damage killer attribution, shared corpse and atomic XP/rare/item save/replay | Full retail treasure mutation tables and recovered numerical rare policy are not claimed; enabled rare profiles require explicit configuration |

The native-emote selector's compiled source fixture exposed a real Unicode
comparison mismatch: .NET ordinal-ignore-case does not fold dotless `ı` into ASCII
`I`. The selector was corrected while retaining the oracle's expected vectors.

## ACE generator implementation (2026-10-07)

The generator domain now follows the pinned ACE profile, queue, count, destination,
notification and lifecycle rules with explicit simulation time and isolated RNG.
Region preparation uses actual DAT geometry and source links/encounters; admission
preflights geometry, actors and owner metadata together. Missing assets remain
held. Defined time has no active source implementation, and normal NPC death uses
the source destruction notification after its death lifecycle. These source quirks
are retained rather than replaced with inferred policies.

Prepared death and wielded treasure now use the original ACE selection tables,
mutation scripts and spell/material/item families, with complete immutable item
properties carried into the existing corpse transaction. Numerical evidence is
recorded in the loot and spawning crate oracle directories. This does not establish
an otherwise unknown retail rare probability; those rates remain configured under
the owner's rare evidence policy.
The literal ACE tables, enum/spell indexes and mutation scripts now compile from
TOML into accepted `.bace` namespace 52. The game child selects one immutable
table set at startup and fails if the record is unavailable; table-set changes
require a graceful child restart. The supplied retail container summary is
compared in `retail-loot-gap-analysis.md`, with no drop-rate retuning claimed.

Transient generated inventory, nested contents and vendor stock have explicit
owner admission. First acquisition and later durable retirement use atomic,
revision-checked, world-epoch-fenced placement transactions. Exact request replay
resolves uncertain outcomes before the simulation releases reservations. This is
BetterACE durability hardening around source lifecycle semantics. Generator queues
and population themselves are transient and rebuild from accepted content.
The bounded cold recovery classifier holds persisted world trees retaining a
Generator IID6 for epoch-fenced retirement before fresh populations spawn. An
exact V4 construction companion exempts its contained Creature/Cow and child
item tree; the cold region owner validates and restores those saved identities,
then reconciles the current generator's profile occupancy before its first tick.
Malformed companions retain their original snapshots and block admission. The
classifier returns complete original snapshots on failure. Mixed transient
creature promotion and cold durable rehydration now have receipt and owner
paths; specialized nested creature valued trees, vendor commerce, crash-active
pet restoration and GamePiece activity remain incomplete.
Fresh generated Creature/Cow acquisitions now select a dedicated bounded save
lane. PostgreSQL rejects a fresh V5 constructed root on ordinary placement,
then the world-epoch-fenced promotion operation validates the full committed
child forest, direct equipped members and nested death-roster parents before
committing an exact receipt. Two real PostgreSQL tests cover rollback, replay,
generic bypass rejection and wrong graph ownership; a runtime save-lane test
proves the generated-save helper selects that operation, and the live inventory
freeze path now selects it for transient constructed roots. A composed actor
transaction across that latter route remains untested. The frozen construction
companion preserves source equipment order, while the relational tree verifies
membership and parentage; it has no separate equip-pass ordinal column.

The generator stash review found no missing generator implementation to restore:
stash `500c62799cde7f6bf6efbdbf9f181b7ad3f7e48a` contains 34 relevant files,
27 identical to the reviewed current versions and seven superseded by current
work. The stash was left intact. Existing geometry ownership was reused.
Full stock-client service composition, platform qualification and representative
performance testing remain separate readiness gates.

Generated creatures prepare source equipment in both equip passes, retain nested
inventory, and use the existing physical projectile owner for ranged attacks with
ACE's non-decrementing NPC ammunition. Death snapshots overlay accepted inventory
quantities and parentage, so transferred items cannot reappear from spawn-time
copies. Equipped enchantments enter the magic registry after the source 0.1-second
delay; teardown waits for registry reservations. Periodic/proc equipment effects
that need additional caster state are rejected during preparation. General combat
consumption of all aura modifiers remains incomplete; registry activation does
not establish complete enchanted-creature combat parity.

Cast sequencing checkpoint: the GDLE frame executor now has 2,400 compiled-original
C++ steps covering exact double-cursor bits, directional hooks and completion,
with the source float quantum preserved. The resolver now has original action/substate/queued-stop source cases; all
6,266 supplied-DAT spell entries / 13,538 selected gestures prepare with no
missing sequence. Connected tests cover rootless sliding, retained cancellation
links, Ready transitions and accepted-style fizzle ordering. These checks cover
animation preparation and the tested controls, not every spell effect. Runtime wall-time admission under
stalls is a separate unresolved source boundary, recorded in `divergences.toml`.
Server motion packet projection matches all 128 non-autonomous original ACE
state-layout fixtures and retains shared-object counters across output rejection.
None of this establishes complete stock-client casting parity.

Unreleased player schema4 preserves cast recovery and contracts. Accepted player
pose/vitals have dirty-age and stale-completion regressions; complete transfers
retain recovery and portal links. NPC save adapters commit one source-defined stage
and its continuation together, preserving earlier stages after later failure.
Workflow-only queued-XP admission changes no player snapshot. These tested adapters
still need the production save/lifecycle/service loop; they are not an assertion
that routine online saving or every offline continuation is fully composed.

Recall/portal integration checkpoint (2026-10-08): command recalls have bounded
cold preparation, destination geometry requests, source-timed authored motion and
canonical private notice projection. Original ACE handler prefixes provide 182
notice/admission vectors; original portal physics methods provide 80 state cases.
The existing real-DAT server test passed with combat-to-Marketplace motion
preparation. These fixtures exclude the complete delayed output transcript.
Portal and known-audience observer output now retain exact reliable admission
obligations, with an explicit generic-visibility fence and lifecycle transfer on
disconnect. The frozen checkpoint passes formatting, strict all-target workspace
clippy, all workspace tests (1,540 passed, 41 ignored), and the architecture check
(51 crates, 1,872 Rust files). Validation used Rust/Cargo 1.96.1, two build jobs,
and a separate target directory with debug information and incremental builds
disabled. Shared-target interference in the earlier run was eliminated; failures
were retained, diagnosed and repaired before this passing run. These results
qualify the frozen checkpoint, not concurrent newer changes in the live tree.
In particular, later spell/portal completion composition remains outside this
checkpoint's integration evidence.

The full run includes the 32-character, four-scenario imbue statistics regression.
The recall, portal-state, binding and NoCorpse original-source fixtures regenerated
byte-for-byte; their method inputs were checked against the pinned ACE git objects.
Binding's 480 cases qualify the existing eligibility/position/stamina subset, not
live binding-object Use. NoCorpse's 108 cases and constructed restoration's 256
cases remain evidence for implementation still to be completed.

Validation also exposed a player-owned creature/emote cast paying mana twice.
The explicit-origin correction is documented in `divergences.toml` as
`gdle-player-emote-double-mana`; it is intentionally different from that GDLE edge
case under BASE-01. The original C++ oracle retains 1,512 upstream cases, including
the double debit; connected tests separately verify the chosen one-charge
behavior, full-pool prepayment, and ordinary player release costs. None of these
checks involved a game client.

The approved gameplay plan remains incomplete. In particular, complete
constructed-creature promotion and remaining generator subtypes, a composed
binding-object regression, remaining portal completion obligations, complete
corpse access/NoCorpse behavior, generic skill device activation effects, and
the remaining compatible staff/social commands still require integration.
Incompatible ACE backend commands
remain excluded; source TODOs remain unsupported. Holtburg remains disabled.
Client testing remains deliberately deferred by the project owner.

Newer live-tree work has since connected authenticated binding-object Use through
range, source motion/sound/message, and durable sanctuary/stamina completion;
generator-created creature V4 restoration now retains creature identity and
pre-spawn occupancy. Inventory has a fresh split-to-wield proposal and receipt-
gated combat equipment modes. Portal output covers additional blocked, aborted,
linked, summoned and completion paths. The summoned visual now uses ACE's
`portalgateway` template 1955; the linked original portal remains the
destination/policy source. A two-template accepted-pack regression passes.
A portal-owned composed output test now follows the accepted gateway blueprint
through Summoned, observer Create, exact reliable receipt, lifetime Removed,
observer Delete and its receipt. It also retains wrong ticket identities and
receipts. The ready blueprint path now rechecks the still-retained Summon
ticket before visibility admission. This uses synthetic accepted observer
views; actual simulation expiry cadence and client rendering remain gates.
A packetless, exact committed summoned completion now releases its ticket after
the gateway enters visibility even if an observer Create is awaiting receipt
or private output is full. Create and later lifetime Remove/Delete still retain
their separate reliable visibility receipts; the focused composed regression
passes. Failed or output-bearing completions keep their existing gates.
A composed admitted-session backend test now covers ordered recall
announcement and motion through hide, teleport,
materialization, reliable private/observer receipts and cancellation silence;
the full live ingress/client transcript remains a gate. Failed or aborted
recall-origin completion retains an explicit unsupported output obligation:
pinned `Player_Location.cs` emits `YouHaveMovedTooFar` before Teleport on the
seven recall actions, but has no post-Teleport failure callback. The tempting
`YouFailToRecallToLifestone`/`YouFailToRecallToPortal` codes are unused enum
entries in the pinned server, so the runtime does not invent those packets.
Connected tests cover the source error before portal work, exact failed-ticket
retention and disconnected cancellation silence. Silent staged/cancelled
recall events now drain without private/observer packet capacity or a surviving
replication owner; the disconnected zero-capacity regression passes.
Player death now freezes the accepted
death action before save latency and projects its source text and canonical
motion. The committed Started output now orders Health, death count, level,
vitae pool, vitae enchantment and purge before the death animation. A bounded
Completed projection emits the exact private pack, split and coin actions for
the supported inventory-loss branch. Held Olthoi nested-container CreateList
children now retain source parent IDs, GeneratorId and final sibling slots in
one frozen death inventory operation. Unsupported equipped, bonded and
pyreal-destroy branches retain their delivery obligations. Corpse V5 preserves
victim/killer/rare/PK permit state; live Open/Close
checks authoritative range and rights, tracks one active viewer, closes on
logout, and defers expiry. Committed corpse and NoCorpse roots are registered
with the visibility owner from cold prepared DAT blueprints; shared corpse
expiry owns later durable removal. Durable consent origins remain in progress.
Native PVE
deaths now carry a separate versioned kind-104 durable receipt, including an
unplaced zero-drop NoCorpse transaction. A real PostgreSQL test verifies exact
receipt replay and rejection of changed or routine writes. The receipt is not a
world item. The PVE NoCorpse path now also prepares exact world roots and a
separate world-drop receipt. Player Bool29 NoCorpse has its own source-selected
death ticket: fresh treasure and eligible preexisting inventory/equipment are
frozen by preserved DestinationType, then a PlayerSaveV6 checkpoint and
ItemSaveV5 world placements commit under one operation before world-root
adoption. The zero-drop case still commits the player checkpoint; an owner
test covers stale pose rejection, exact replay and delayed respawn; the real
PostgreSQL zero-drop checkpoint replay test also passes. A direct, noncontainer
selected pack root now completes only after exact V5 origin, revision and
world-placement checks; source emits no private dequip packet for that branch,
and WorldDrops owns its CreateObject visibility. A single eligible equipped
NoCorpse root now preflights the observer projection, confirms exact V5 origin,
revision and copied world pose, then emits the source dequip sound, ObjDesc and
Wielder clear with private burden before WorldDrops CreateObject. Pinned packet
order and tampered durable receipt tests pass. Multiple equipped roots,
equipped containers, item effects, disconnected output and several corpse
branches remain delivery gates;
these paths do not imply complete PVE output or death parity.
For ordinary outdoor corpses, committed Completed output now checks the V5
corpse placement and V6 player death state against the frozen copied pose,
then sends ACE's private LastOutsideDeath position update (0x02DB,
PositionType 14) before the conditional corpse-location chat and source
retained-item notices. Indoor and Olthoi deaths do not send that position;
focused source layout, sequence/capacity and committed-pose tests pass.
The default destroyed-pyreal branch now checks the exact saved V5 coin source
rows (whole removals or partial stack remainder) before Completed delivery.
It emits the pinned coin spend, burden and CoinValue sequence, and counts those
coins in the source loss and outdoor location messages even though no coin
object enters the corpse. Missing or mismatched receipt rows retain the output
obligation. Focused simulation and runtime regressions pass; other corpse
completion branches remain held.
Corpse Open now rechecks the exact committed root and direct/nested item rows
after adoption and before ViewContents. Changed children refresh retained DAT
preparation without consuming another permit; missing/root-changed sources and
excessive churn retain and block output. A stale nested version/placement
regression and strict targeted Clippy pass. Generic world-container Open and
other corpse branches remain held.
The bace-world player NoCorpse preflight/adopt boundary now validates copied
accepted poses, geometry, identity and capacity for all world roots before
atomic anchor insertion, retaining the dead player for respawn. Its focused
zero-root, invalid-root and successful-root regression passes. The restart-only
`world.death.creatures_drop_createlist_wield` option is explicit and defaults
false; config tests cover both values. The world boundary only adopts roots
after the exact durable player/item receipt.
ItemSaveV5 now defines nullable immutable source DestinationType flags and
tests legacy unknown migration. Native CreateList acquisition and live item
freezers, login, online save baselines, and the principal gameplay writers
carry that origin; a PostgreSQL transition test rejects altered or erased
origins. Older saves retain unknown origin and cannot be guessed from current
placement. NoCorpse selection rejects unknown origin rather than inferring it
from current placement.
An ignored approved-asset regression now runs production corpse-spill
preparation against actual 0x8602 outdoor DAT geometry, Pyreal shape 273 and
the accepted ACE table set. It verifies the source-relative +0.05 placement,
accepted physical body, V5 visibility revision and encoded CreateObject identity.
This qualifies that presentation path without a game-client claim.
The direct-pack NoCorpse Completed path now admits deterministic same-lane
sibling slot shifts only after exact +1 revision and committed V5 placement
checks. A selected container now retains its nested Contain tree under the
same world root: descendants receive same-placement +1 revisions in the one
death operation, so the online dirty owner can acknowledge the full hierarchy.
Source-cache and Completed gates verify each committed descendant's parent,
slot, revision and V5 origin. Simulation, runtime and real PostgreSQL tests
cover positive replay and stale-child rollback. BetterACE corrects ACE's
retained inventory/world duplicate by sending the reserved post-transfer burden
as a canonical private PropertyInt 5 after the durable receipt; absent recipient
or output capacity retains completion. The deliberate source discrepancy and
positive/negative regression evidence are recorded in `docs/divergences.toml`.
Other equipped NoCorpse cases and ordinary world-container Open remain separate obligations.
Shop stock now has a frozen V1 unplaced marker and a relational vendor mapping.
Its world-epoch/vendor-version fenced PostgreSQL operation commits the marker
and item forest together, rejects missing descendants without a journal
receipt, and replays exact requests. A bounded cold loader checks the complete
ordered V5 forest. Authenticated NPC Shop Use now fences source/version/cell and authoritative
range, reserves source-ordered stock under the simulation owner, commits the
exact marker and item forest, confirms the receipt, and admits a private reliable
ApproachVendor listing. Restart now reads the V5 vendor source and ordered stock
forest under one hierarchy lock, avoiding a torn source/marker view. Pure
default stock at later marker versions reconstructs its base items while
retaining the advanced durable marker version/revision; repeat Use checks both.
Unique contributions, mutated stock rows and wrong revision deltas remain held.
Focused owner/runtime tests cover those steps,
but the fixture lacks a full DAT-backed authenticated actor-range transcript.
Buy/Sell and alternate currency remain unsupported. Transient-only vendors are
held until a durable vendor parent exists.
The economy owner now has a bounded default Buy quote for accepted stock IDs,
templates and revisions. It preserves request order, partitions stacks and
rejects price overflow; three focused quote tests and strict Clippy pass. Live
Buy still needs joined player/vendor durable state and retained receipt/output.
The source's minimum
price of one rules out a zero-currency shortcut.
The inventory owner now proposes an exact server-selected coin-stack debit and
prepared fresh leaf grants in one atomic inventory change, rejecting shortage,
wrong ownership, duplicate identities, capacity and burden. An ACE-order coin
selector walks direct stacks before side containers. The simulation owner now
retains an authenticated default Buy reservation with source, stock and marker
fences, one claimed inventory proposal, exact joint receipt admission and
definite-rejection release. Generic inventory retry/reject/confirm cannot bypass
the pending Buy. Focused inventory 5/5 and simulation 3/3 tests plus strict
Clippy pass. This remains a prerequisite: live Buy still needs joined player
CoinValue/vendor counter and item save, exact durable submission and retained
receipt/output. A real PostgreSQL composite Buy regression exercises one
operation across vendor counters, player V6 currency/burden, coin V5 debit,
fresh V5 grant and marker CAS, including rollback, replay and atomic cold read.
The simulation command boundary now carries bounded default Buy Reserve,
Reject and exact Confirm actions through its worker decision channel. The
reservation stays held on a stale receipt and releases only on its matching
joint receipt or definite rejection; six focused command/owner tests and
strict targeted Clippy pass. Live Buy ingress remains closed: the joined
player snapshot/critical lease, vendor and item freezer, uncertain save retry,
online baseline and reliable source output are not connected.
The Pet output owner now clears a disconnected, definitively rejected summon
only when its pending Saving phase, actor, device and rejected completion match;
a mismatched result remains retained. Its focused backend test and strict
runtime library Clippy pass. Mixed passive/CombatPet devices, cold CombatPet
equipment and crash-active pet restoration still lack their shared durable
identity/lifetime and two-device transaction contracts.
AttributeTransferDevice now has source-property preparation, authenticated Use
and type-3 confirmation, exact character plus item receipt, and private
two-attribute update/consumption projection. Focused simulation, packet,
mixed frozen-save and composed prompt-to-committed-output tests pass, including
retention across disconnect.
Skill devices now prepare authored SharedCooldown and positive CooldownDuration,
check cooldown eligibility under the simulation owner, and freeze the exact
before/after registry in the same durable character/item operation as the skill
change. Confirmed completion publishes the private cooldown enchantment; focused
rejection, replay, save/reload and output tests pass. Mixed generic activation
requirements retain their full eligibility checks. Pinned Use plus Talk skill
devices now admit bounded String17 and retain source-ordered ActivationTalk chat
between confirmation and UseDone; five focused runtime tests, including packet
order, and strict targeted Clippy pass. Emotes, activation targets and other
response flags remain held across skill and attribute devices. Pinned Int119
Active=0 on an authenticated skill-device Use now checks the accepted item
revision, skips cooldown and all activation effects, and emits only UseDone;
focused simulation/runtime tests and strict all-target Clippy pass.
Sentinel `@boot` now has typed source selector parsing, online character/account/
IID resolution, source private result, final AccountBoot packet and session
termination, plus retained external Audit feed publication. Focused parser,
runtime-dispatch and strict targeted Clippy checks pass; a complete live boot
transcript remains untested. Developer `@whoami` now has typed parsing, authenticated Developer/self
inspection, the pinned eight-digit hexadecimal GUID line and existing private
reliable Broadcast output. Focused parser, owner and runtime dispatch tests
pass; it is distinct from Envoy `@myiid`. Developer `@listplayers` now
rechecks entered session generations and Developer authority, projects the
pinned access-filtered roster and total through private reliable output, and
bounds response chunks. Focused parser and connected owner tests cover stale
sessions; stock-client playback remains unqualified. Developer `@gps` now
uses the authenticated inspection route to read accepted world cell, position
and yaw, then emits the pinned one-line private Broadcast. Parser, simulation
owner and runtime dispatch tests plus strict targeted Clippy pass; arbitrary
quaternion/locale presentation and stock-client playback remain unqualified.
The broader catalog still has
unimplemented compatible families. Pinned `@show-allegiances` is held because
its console-only full-chain output needs a complete allegiance-registry snapshot and host-console
record owner that are absent from the actor-local panel path.
Account-ban storage now has revision-fenced Ban/Unban/Expire operations with
exact replay, bounded active-ban pages and explicit expired-ban clearance. The
login worker checks the durable verdict after password proof, clears expiry
before entry and sends a banned account's reason/remaining seconds only after
the cookie handshake, without acquiring account ownership. Two real PostgreSQL
ban tests and focused authentication/network tests pass. Sentinel `@ban`,
`@unban` and `@banlist` now parse typed source inputs and retain the exact
revision-fenced write through its receipt. After a committed write, simulation
admits an authoritative in-game Audit event before private result and target
AccountBoot output; focused parser, simulation and runtime tests pass. Issuer
detach before Audit admission leaves the exact committed phase retained for
recovery. The external Audit publisher now reserves bounded capacity before
assigning a feed sequence; Full retains the exact event and recipients for
retry, while Closed surfaces degraded failure with a recoverable event. Its
admin and runtime focused suites pass 4/4 each. This does not establish a
complete stock-client command/login transcript.
Direct simulation social-presence handoff now preflights and performs the same
fellowship departure as logout, removes invitations involving the departing
actor, and clears panel/allegiance listeners before unregistering. Focused
social lifecycle tests pass 9/9. Other offline and reward-origin social paths
still require connected review.
Allegiance login XP redemption now submits once per entered session generation,
correlates the simulation control, and marks a reward handled only after the
exact allegiance service durable completion. Busy and rejected outcomes retry;
disconnect clears the generation state. Focused runtime login tests pass 2/2.
Other reward origins and full client transcripts remain open.
The prior frozen workspace checkpoint above must not be read as validation of
these newer live-tree changes; full current-tree QA is still pending.
