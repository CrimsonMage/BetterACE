# bace-world

Status: foundation.

Single-owner synthetic and prepared geometry scenes, entity registry, read projections and atomic validated teleport. Complete authentic landblock/cell admission and traversal remain unqualified.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.

World now also owns registered combatant health/profile state beside its actor/physics registry. Combat geometry checks read the single accepted Body state and fail closed on missing/cross-cell geometry. Full live world admission remains gated.

World also owns synthetic door collider identity, location and solid state.
Occupancy uses the same actors' accepted Body poses and radii, including players
and NPCs, excluding the door itself. Door-use visibility ignores only the targeted
door's own collider; other obstacles remain authoritative. All currently admitted
synthetic Bodies participate in collision; unsupported authentic ethereal/parented
state is not silently inferred from content. Actual DAT admission remains gated.

World now exposes immutable actor-state/resource projections and atomically
prechecked vital batches. Health remains solely in Combatant; mana/stamina must
be explicitly prepared and cannot be synthesized by a cast. All participants,
resources, revisions and contributor capacity are checked before any batch
mutation. Projectiles occupy a separate world-owned identity/physics map and
use accepted actor positions for collision queries. Scalar entity-property
proposals retain revision checks; none of these new paths grants client poses.

`install_geometry`, `prepare_geometry_body` and `validate_actor` now connect the
immutable authored-geometry path to the existing actor registry and tick owner.
Insertion, teleports and generation adoption validate placement before mutation.
`insert_loading_player` is the narrow player-entry exception: it still validates
identity and authored static placement, while allowing a second loading player
at an occupied spawn. Pinned ACE `Player.InitPhysicsObj` sets IgnoreCollisions
through the entry pink bubble; `OnTeleportComplete` clears it. Ordinary actor
insertion keeps its dynamic-overlap check. The world owner regression verifies
shared entry, ordinary rejection and static-wall rejection; stock-client entry
remains unqualified.
A reusable bounded dynamic-collider buffer includes prepared actor spheres or
cylinders; projectile queries use those same accepted actor shapes and geometry.
Missing geometry blocks an actor and emits `take_geometry_blocked` rather than
terminating every actor's tick. The old synthetic scene path remains separately
explicit. This integration is still gated on complete asset/admission preparation
and authentic trajectory qualification; it is not a declaration of playable AC
movement. `validate_vital_batch` exposes the same pure preflight used by atomic
vital mutation, allowing projectile launch to validate resource debits first.

World now owns generation-correlated server motion controllers and bounded hook/
completion output. Completed cyclic playback remains owned until replacement or
explicit safe retirement. Pending events prevent retirement. Rootless player cast
chains preserve authorized locomotion; nonzero-root grounded cast sliding is
explicitly rejected rather than combining client pose or unqualified root motion.

Mana reservations retain resource ownership while component/portal commits are
pending. Only Mana can be reserved; health/stamina combat operations continue.
Ordinary vital batches reject foreign reservations before any mutation; matching
tokens permit receipt adoption and require explicit release. Actor removal/corpse
replacement refuses reservations. Trusted direct Combatant mutation is outside
this gate, so callers must continue routing Mana changes through World batches.

`has_pending_action_motion` reads full `CM_Action` bits on retained completion
records. It is the motion-specific attack/cast arbitration query; generic
`motion_busy` also includes Ready and substate transitions and must not substitute
for it. An ending cast's positive substate/Ready links permit a new cast to append
without discarding those links. Pending actions still block that admission.

## Authoritative PVS

`PreparedCellVisibility` retains actual EnvCell SeenOutside and VisibleCells data
with the admitted geometry generation. Publication validates bounded, sorted cell
identities and closed PVS references before changing metadata. Missing metadata
fails a query; it never disables collision or invents outdoor visibility.

PVS queries follow pinned ACE ObjectMaint: outdoor observers use current/adjacent
landblocks and SeenOutside interiors; indoor observers use their own/visible cells
and an outside bridge only when authored. Queries index entity identities by cell
and read current accepted Body/projectile positions. They never copy mutable poses.
Cell/identity mutations invalidate the index; ordinary same-cell movement reuses
it. Reusable candidate buffers return errors without a partial published view.
Per-session knowledge and the initial-distance clamp belong to replication.

`oracle/visibility_generate.py --source /path/to/pinned/ACE --dotnet /path/to/dotnet`
verifies official source bytes and compiles original ObjectMaint/Position methods
against synthetic cell/container adapters. Thirty PVS/distance vectors and twenty
replication clamp/expiry vectors qualify those policies; they do not establish
complete object packet projection or stock-client readiness. The runtime's ignored
`visibility_assets` test compares admitted metadata with fingerprinted raw EnvCells.
The bounded query sample covers 2,000 actors and 128 observers, reusing one output
buffer across 1,280 queries; timing is local evidence, not a shard-capacity claim.
Original GDLE `TryBeginCast` gate and queue methods qualify the distinction in
`gdle_sequence_queue`; other cast, portal and durability checks remain separate.

`drain_motion_callbacks` handles only zero-count successors appended by a consumed
server completion in the current physics phase. Authorization is epoch/token fenced
and one-use; initial input and replays cannot complete motion early. The global
256-call phase budget and output capacity retain excess work for a later tick.
Nonzero successors are not advanced or initialized by this callback drain. Original
server `OnMotionDone` reentrant C++ cases qualify same-update zero callbacks.

Projectile steps now validate collision, frame transfer and range-frame metadata
before committing any state. `step_projectile_filtered` lets the gameplay owner
exclude source-ineligible actor contacts while static geometry stays mandatory.
`projectile_distance_from` uses accumulated admitted portal translations back to
the launch frame. `actor_in_frame` uses verified RegionLand metrics for cross-block
positions; accepted velocities already use global axes. Missing metrics/cells fail
explicitly. Damage permission, launch leading options, source-specific destruction
and output remain with combat/magic owners. Single-portal traces do not qualify
whole-world projectile traversal or moving compound projectile geometry.

Each admitted actor can reserve four immutable locomotion style slots. Equipment
replacement keeps the current Body asset alive until an authored transition, and
cannot lose its reserved capacity while persistence is delayed. Explicit teleports
restore the previous accepted style at Ready with controls cleared, including
portal-space stops; respawn can explicitly select its policy style. Stale-epoch
animation evidence is excluded from object views.

Death programs are cold-prepared StopCompletely→Motion_Dead chains. World starts
cached programs on accepted health zero independently of database latency, keeps
positive in-progress links and their callbacks, disables manual controls, and
continues swept root/gravity execution. Lifecycle completion reads the trusted
Death token and epoch. Up to sixteen prepared source-state variants per actor and
4,096 actor slots are bounded; replacement retains the actual old style during
an equipment transition. Client animation acknowledgments cannot complete death.
Missing death assets stay blocked; unlike GDLE's immediate completion fallback on
animation admission failure, they never fabricate an animation-done callback.
Death admission uses a bounded round-robin sweep, and its already-admitted cursor
and gravity continue during region draining so corpse completion cannot deadlock
behind dormancy.

`begin_style_action` admits an authored style transition and immediate action as
one bounded operation. Both source-state transitions, token ownership, playback
capacity and destination locomotion controls are checked before mutation. It
retains both FIFO completion tokens in the existing playback owner and changes
interpreted style on admission, so gameplay timers need not await a style callback.
The integration tests cover callback order and atomic invalid/busy rejection;
the existing GDLE style/queue oracles qualify the underlying chain operations,
not a complete recall or equipment packet transcript.

Player NoCorpse world-root admission has a separate preflight/adopt pair. It
checks a dead player, copied accepted pose, unique identities, capacity and
static placement for every root before inserting any anchor, while retaining
the player body for respawn. The durability ticket and loot graph remain owned
by the death subsystem; this world boundary does not claim complete NoCorpse
integration.

Selected-creature health observations are a bounded world-owned lane (4,096
subscriptions and retained notifications). Damage and vital batches reserve all
required notification slots before mutation. Each record freezes the accepted
intermediate pool and recipient binding, so damage followed by healing cannot
collapse into a tick-end value. Combat hooks retain their strike/contact while
that lane is full. Recipient removal waits for its pending records; a retired target’s frozen ID
and pool remain deliverable and its identity stays reserved until dequeue. Selection changes
stop future subscriptions without deleting already accepted output. Direct
`combatant_mut` remains a maintenance accessor and must not bypass World damage
or vital batch APIs for live health mutations. Two owner tests cover intermediate
values, subscription retirement, whole-batch rejection and exact retry.
