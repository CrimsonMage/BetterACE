# bace-physics

Status: foundation.

The legacy synthetic swept-sphere/box harness and the connected prepared-geometry solver both retain private accepted state; untrusted observations cannot overwrite it. Full AC transition/terrain/portal qualification is still outstanding.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.

`SyntheticScene::segment_clear` adds a conservative checked obstacle query for synthetic melee tests. It rejects invalid/outside endpoints and does not establish DAT/BSP combat visibility.

## GDLE scalar cell-local query foundation

`GdleBspCell` now supplies source-backed collision **queries** independent of the
synthetic `Body` integration. `GdlePolygon::prepare` preserves the authored polygon
and computes its plane with GDLE's fan-normal sum and average plane distance;
it does not replace the polygon with an AABB or a generic triangle proxy.
`intersects_solid` and `sphere_contact` port the pinned GDLE BSP node/leaf ordering,
sphere bounds checks, `polygon_hits_sphere_slow_but_sure`, and `pos_hits_sphere`.
A contact while moving away remains distinguishable through `approaching=false`,
matching upstream's non-null polygon side effect with a false hit result.

`sample_path` uses GDLE's non-viewer displacement/radius subdivision and these
queries to report the first colliding sample. It is **not** a time-of-impact,
sliding, stepping, gravity, cell-transfer or accepted-position solver. Initial
placement must be checked separately, including zero-length paths. An isolated query does not qualify accepted movement. The connected GeometryRegion
path below consumes these primitives; authentic entry still requires complete
asset and placement admission.

Preparation rejects cycles/shared children, unreachable nodes, missing polygon
indices, unsupported malformed/nonplanar/nonconvex polygons, nonfinite inputs,
invalid used bounds/planes, and excessive counts/depth. The admitted scalar
coordinate range is +/-1,000,000; polygon plane/convexity tolerance is 0.005 local
units; split-plane squared normal length must be 0.999–1.001. These are explicit
hardening gates, not rewritten asset coordinates. Used sphere radii are validated;
empty leaves retain finite unused filler fields because GDLE returns before
reading their bounds when `num_polys == 0`. Actual DAT empty leaves contain
`0xCDCDCDCD` filler, which must not become a false collision requirement.

Queries allocate no heap memory and consume an explicit `BspQueryBudget` shared
across the whole path query. The default caps node visits and polygon tests at
262,144 each and samples at 1,024; validated tree depth is at most 128. Exhaustion
returns an error, never an open path or partially accepted state. Preparation
and asset conversion belong off the simulation thread.

Provenance: GDLEnhanced `353cbab52ef7da2b7063bc3e3f008461d8531693`,
`Source/PhatSDK/{Polygon.cpp,BSPData.cpp,Transition.cpp,MathLib.cpp,MathLib.h,PhatSDK.h}`,
AGPL-3.0-only. `tools/bace-compat/oracle/gdle_bsp_generate.py` verifies each input
against that immutable Git commit and compiles verbatim collision/subdivision
methods plus the original scalar Vector implementation. Harness declarations
and a same-cell Position-delta adapter supply dependencies; no tested collision
predicate is replaced. Synthetic vectors cover 177 placement/contact cases,
18 subdivision cases, tangencies/corners, moving-away contacts and empty leaves.
C++ compilation disables fast-math and FP contraction. This qualifies the scalar
f32/f64 operation sequence on the tested Linux build; it does not establish x87,
SIMD, FMA, or cross-platform bitwise equivalence.

Actual approved portal DAT preparation passed for all 3,168 cells in 772
Environment models, with 9,504 bounded placement/contact query pairs. Tests
exercise actual immutable geometry without committing proprietary bytes; these
queries are not stock-client trajectory or capacity qualification.

```sh
cargo +stable test -p bace-physics --test gdle_bsp
cargo +stable test -p bace-compat --test gdle_bsp_vectors
BACE_DAT_DIRECTORY=/path/to/DATs cargo +stable test -p bace-compat --test gdle_bsp_vectors supplied_real -- --ignored --nocapture
```

Synthetic scenes now have bounded dynamic obstacle slots and server-owned solid
flags. Body movement reports the first physically swept dynamic obstacle only
when it is not occluded by an earlier static hit. The simulation uses those
contacts for GDLE-style NPC door activation; a client's contact report is not an
input. Static AABB fixtures are distinct from the source-backed BSP query work.

`Body::spawn_oriented` admits explicit initial heading and angular capability.
Manual turn intent and generation-fenced server turning update the same accepted
state; no reported heading setter exists. The legacy synthetic spawn constructor
has zero angular capability, so unprepared turning cannot silently succeed.
`tests/turning.rs` covers rate bounds, manual cancellation and teleport epochs.

`ProjectileBody` ports the pinned GDLE airborne scalar integration branch and
initial velocity assignment: the 50-unit speed ceiling, epsilon comparison,
0.25 small-velocity threshold, zero-velocity branch and ballistic operation order.
`gdle_projectile` compares 3,240 exact-f32 steps from compiled original C++ scalar
methods, plus 48 `LandDefs`/`Position` frame offsets. Quanta are explicit positive
f32 values through 0.2; no wall clock or client hit report enters physics. Invalid
contacts/steps retain state. Generic lifetime bounds also expire below f32 time
resolution; gameplay owners implement their separate source Destroy/rot schedules.

World queries include admitted static geometry and accepted actor spheres or
cylinders, source-owned eligibility filters, and a validated single portal transfer
per quantum. A failed trace retains body/cell/frame state. Multi-portal/corner
traversal, moving projectile compound shapes, full collision response and ground
friction remain unqualified. GDLE computes a ballistic endpoint then checks the
chord through `CTransition`; this is not a continuous curved-path or homing model.
`CollisionShape` now separately retains authored Setup nominal radius/height for
launch/aim calculations; collider extents are not interchangeable with them.

Read-only retail decompilation review followed its INDEX/AGENTS.md workflow:
five summaries, foundation manifests 04/03, Vector3 leaves, then scoped
CPhysicsObj::UpdatePhysicsInternal (00510700). It corroborates the speed/threshold
and integration branches. Its x87 temporaries are not claimed bit-equivalent to
these GDLE f32 vectors; proprietary source is not embedded in the repository.

## Connected geometry solver work

`GeometryRegion`/`GeometryCell` admit immutable authored polygon faces, cell
half-spaces, portal links and static BSPs. `Body::spawn_geometry` and
`step_geometry` integrate continuous sphere face/edge/vertex casts, bounded
sliding/stair trials, gravity, grounding, explicit frame transfers and dynamic
sphere/cylinder contacts. A failed query returns no accepted body state. Each
query is work-bounded; no scene query opens files or waits. The world retains this
same Body and reports geometry failures per actor. Prepared object collision can
be disabled/re-enabled for doors while immutable face data remains shared.

`GeometrySpawn` requires a prepared collision shape. Geometry bodies reject raw
jump flags; only an explicit character-authorized jump may consume the jump
impulse. Motion controls use prepared DAT cycle velocities and authoritative
Run/burden projections. Legacy `SyntheticScene` remains an explicit test path.

Cold static placement now resolves an authored indoor cell hint using the primary
collision sphere center and that cell's ordered VisibleCells candidates, following
pinned GDLE `ObjCell.cpp::find_cell_list` and
`EnvCell.cpp::find_visible_child_cell` at
`353cbab52ef7da2b7063bc3e3f008461d8531693`. It keeps the authored pose
and still requires the destination cell and strict collision placement. The
ignored approved-DAT regression in `bace-runtime/tests/region_geometry.rs`
checks four Patches v0.9.295 static roots in landblock `0x8602`: three portals
remain in their authored cells and one generator resolves to an authored visible
neighbor. This is source-derived admission evidence, not full movement parity.

The new continuous solver's geometric tests cover thin walls, tangent/sliding
motion, dynamic actors, portal-neighbor contacts, refused geometry, grounding,
jumps and failed teleports. These tests are not evidence of complete ACE/GDLE
transition parity. Whole-region static/setup composition, multi-portal traversal,
restrictions, remaining physics-state collision flags, full root-animation integration and
stock-client trajectory acceptance remain required before gameplay readiness.
