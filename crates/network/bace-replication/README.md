# bace-replication

Status: foundation. Bounded per-object sequence counters and per-observer knowledge
are implemented; complete authoritative object descriptions,
movement scheduling and complete world-to-wire projection remain unsupported.
Frozen accepted World motion can be mapped to the pinned server movement layout;
the caller's canonical sequence owner assigns command numbers before reliable
private and observer publication. This supports death motion without deriving
movement from a client pose.
The inventory projection can also assign the actor's private Position property
sequence and encode 0x02DB for committed outdoor corpse LastOutsideDeath output.
Its focused test checks the sequence and retained capacity boundary.

`Sequences` follows pinned ACE `Network/Sequence/{SequenceManager,ByteSequence,
UShortSequence}.cs`: byte properties begin at 0 on first increment, object counters
at 1, and motion at 2 with its reserved high bit. Official C# boundary vectors are
checked by `tools/bace-compat/tests/replication_sequences.rs`. Tuple keys avoid
upstream's possible property/type bit overlap; table capacity is bounded.

`Visibility` is an adapter-driven knowledge tracker, not a spatial-query engine.
Delayed forget tickets carry generations. Re-observation cancels stale removal;
after committed removal the next observation requires a full create. Tests cover
the user-reported recall/forget race and both observer directions. This deliberately
prevents a server-side visibility bug; it does not establish retail cadence parity.
Reliable output must retain create/remove messages or close the affected peer.

`SpatialVisibility` now consumes bounded authenticated World PVS snapshots. The
source 112.5-unit 2D clamp applies only to previously unknown objects. Occlusion
retains knowledge for 25 seconds without resetting the original deadline; re-entry
before expiry cancels the pending forget. Expiry precedes admission at the exact
deadline, so a qualifying re-entry gets a fresh create. Actual entity destruction
has a separate immediate retirement path.

Every delta is staged with a unique receipt ticket. Knowledge remains unchanged
until the complete exact remove-before-create batch enters reliable output; retries
retain the same pending delta. Commit returns the candidate buffer for reuse.
Bindings, observer epochs, tick order, lengths, finite distances and capacity are
checked before knowledge changes. Twenty vectors compile the original pinned ACE
ObjectMaint clamp/destruction methods independently; World owns the companion PVS
and Position.Distance2DSquared oracle. Runtime connection/packet qualification
remains distinct from these source-policy and output-admission tests.


`ProgressionProjector` now emits the primary private AvailableExperience update
before its attribute/vital/skill update, matching pinned `Player_Xp.SpendXP` and
`Player_{Attributes,Vitals,Skills}`. It requires a matching authenticated binding,
non-stale revision, fresh action sequence and complete authoritative trait details. Valid zero-XP actions retain their revision and still produce both primary messages; action-sequence fencing prevents replay. Missing details, invalid
views and counter capacity fail before any sequence/revision changes. Multi-key
sequence admission is atomic; a returned ordered batch remains caller-owned until
reliable output admission. The runtime's SendBatch admits all messages before
peer polling or closes the unrecoverable peer on partial failure.

The existing official message/counter fixtures establish the constituent bytes;
`bace-runtime/tests/progression_flow.rs` exercises UDP → typed dispatch → single
simulation owner → primary projection → reliable UDP. Rank-up sound/chat and
the max-rank observer effect have separate source-backed output. Ranked
Endurance now reserves XP, Attribute and Vital counters atomically and encodes
the full private Health follow-up after that rank announcement; a rejected
projection advances none of those counters. Other derived vital/run-rate
effects and stock-client playback remain unqualified. A primary packet batch
alone does not establish the complete ACE action or save transaction.

`EventSequencer` now owns the shared per-session event counter across initial
login and combat notifications. It encodes and bounds a complete batch before
advancing counters, fences the authenticated generation and leaves rejected
projections unchanged. Returned batches retain exact per-message queue IDs and
remain caller-owned until reliable admission. Its initial ordering follows
pinned `Player_Networking.SendSelf` and `SendInventoryAndWieldedItems`: description,
titles, friends, player-create, self-create and ordered possessions. The original
methods are extracted verbatim into the C# oracle, with synthetic projected
inputs and no contracts; `bace-compat/tests/login_projection.rs` compares actual
batch queue/order/counter output to this independent trace. Smartbox output uses
queue 10; UI events use queue 9. This is projection of supplied state, not asset
construction, privacy selection, spatial visibility or a world-readiness claim.

`EventSequencer::project_magic` shares the existing authenticated event sequence
with other event families. It advances only after the entire bounded batch has
encoded successfully; failures retain the previous counter. `tests/magic.rs`
covers failed admission and wraparound. Spell outcomes are not durable success
merely because a packet batch encoded.

`project_server_motion` emits a server-owned F74C motion on Smartbox queue 10.
It advances the shared object's movement/server-control counters and each action
counter only after complete bounded encoding succeeds. Callers retain the exact
result for reliable retries and broadcast it to all observers; projecting again
per observer would incorrectly consume counters. The action high bit remains
clear for these server-initiated motions. This does not authorize or complete a
cast, integrate client movement, or establish stock-client animation playback.
`bace-compat/tests/server_motion_projection.rs` compares all 128 non-autonomous
state combinations to the existing compiled-original ACE serializer fixture;
`tests/server_motion.rs` checks failure atomicity and action-counter wrap.

Accepted social/group events retain their explicit delivery route and recipient
list. Projection maps legacy/Turbine chat, private tells, fellowship/allegiance
profiles, confirmations, friends, squelches and AFK without guessing channel IDs
or permissions. Whole-batch validation precedes shared event/property counter
advancement. Staff inspection/heal/teleport reuse these authoritative sequence
owners and existing portal output phases. Caller retains each exact ordered batch
until reliable admission; projections are not durable success acknowledgments.

`item_experience` projects the source item-owned private Int64 sequence, optional
level-up message and AetheriaLevelUp observer script as one bounded batch. Eleven
vectors compile the original pinned ACE message serializers independently; tests
also verify failed batch admission consumes no property sequence.

Character XP events project only after durable adoption, using the same session
property/event counters as other output. Five complete ordered-byte fixtures are
compiled from pinned ACE `UpdateXpAndLevel`/`CheckForLevelup` and original message
serializers, covering ordinary XP, credit/no-credit level-up and maximum-level
quest notices. Rejected batches leave every counter unchanged. Staff spellbook
projection preserves source add-spell ordering and does not invent a removal
packet where ACE only sends system text.

`ProgressionCursor` keeps only binding/revision/action fences and borrows the live
session's canonical `Sequences`. The standalone `ProgressionProjector` remains a
compatibility wrapper for isolated adapters/tests. Entry, progression and skill
updates therefore share counters; rejected projections cannot advance them.
`project_training_notice` matches 440 original C# handler outputs across all 55
source skill IDs, both outcomes and four credit boundaries. Regenerate with
`oracle/training_notice.py --repo <ACE git checkout> --dotnet <dotnet>`; the script
reads the pinned Git objects, executes the unchanged handler/Skill.cs, and records
source hashes beside the fixture. Approved divergence #14 still controls the
public skill-class update; notices do not claim complete rank-up action parity.

Skill-device success joins the exact accepted inventory consumption into one
atomic canonical-counter projection. Lowering publishes RefundXP before the full
private skill, credit and source notice; augmentation places consumption between
the skill update and augmentation property/XP/notice/script/local chat. It does
not reuse the ordinary training class-update sequence. Ten traces execute the
unchanged pinned SkillAlterationDevice.AlterSkill, Player_Skills lowering methods,
Player_Xp.RefundXP and AugmentationDevice.DoAugmentation against synthetic state
and transport boundaries (`oracle/skill_devices.py`). Existing wire fixtures cover
the constituent serializers; these traces cover ordering and notice selection.
Failure/size tests verify no actor, item or session counter advances on rejected
composition. Source enum skill sentences are used for prompts and notices.

Equipment batches use the character's existing visual/vital counters and each
item's existing position/property counters. ParentEvent uses the wearer's instance
sequence and the child's next position sequence, as pinned
`GameMessageParentEvent` requires. `tests/inventory_equipment.rs` records source
Parent/Pickup byte vectors and rejects whole batches without counter movement.
The runtime retains accepted model/attachment and observer obligations separately
from private session delivery; this is serializer/ownership coverage, not a claim
of complete equipment action trace or client qualification.

Accepted object views now project authoritative position, upright heading,
velocity, current source motion, queued action rates, and bound server move/turn
goals. One per-object counter owner produces bytes shared by recipients. A
bounded ten-counter preview prevents failed encoding from consuming counters;
property counters are not copied. Source style/substate/Dead commands are fields
of the movement state rather than fabricated action-list entries. Frozen retirement
presentation preserves the real prior motion and velocity instead of inventing idle.

Target-query output uses the existing canonical EventSequencer and UI queue9.
Clears/missing targets produce no packet and consume no sequence. The projector
commits the event sequence only after the complete bounded packet batch encodes;
wrong-session and pressure regressions preserve the previous counter.

AppraisalProfile now has a separate bounded queue-9 projector using the same
authenticated session event counter. A failed codec or batch preflight leaves
that counter untouched. This is a packet boundary only: live Identify and NPC
wake/output composition remain unsupported.

The inventory projector now encodes a private `ApproachVendor` listing inside
the same bounded batch as pickup sound and UseDone. Its queue-9 listing consumes
the canonical session event sequence before UseDone. Focused tests verify order,
exact bytes against the independently qualified listing codec, and rejection
without counter advance. Live Buy still needs its joined durable/output owner.
