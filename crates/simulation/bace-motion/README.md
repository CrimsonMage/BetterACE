# bace-motion

Status: foundation.

Finite bounded intent and server capability contracts. Authentic AC motion-table interpolation and client reconciliation remain unimplemented.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.

`TurnIntent` is a bounded legal axis, not a client-selected orientation.
`TurnControl` identifies a server controller generation. Physics integrates
accepted heading from prepared angular capability; player motion/turn intent can
cancel an automatic turn without assigning a pose. `heading_delta` supplies the
shortest scalar angular difference. This does not implement full DAT motion-table
blending or qualify authentic client movement.

`PreparedMotion` now compiles bounded half-open animation segments with signed
playback, authored hook directions, exact frame offsets and full neutral hook
payloads. Its cursor preflights output capacity and advances atomically, so output
backpressure cannot skip or repeat an admitted hook. `LocomotionProfile` interprets
bounded movement controls using prepared cycle/modifier velocity and omega;
backwards, sideways, running and turning adjustments follow the pinned
`MotionInterp` methods. Runtime performs DAT lookup/preparation off-thread.
This is not yet complete transition-link blending, airborne interpretation or a
qualified stock-client motion transcript.

`RootCycle` ports upright ACE `Sequence` root-frame stepping. Its independent C#
oracle covers 1,800 steps, including one-time multi-clip warmup: the harness now
sets `FirstCyclic = Last`, matching `Sequence.append_animation` (lines203–215),
rather than incorrectly repeating the whole list. The correction changed expected
trajectories; cursor checks and the existing numeric tolerance were retained.

`PreparedMotionChain` and `MotionPlayback` add bounded GDLE server action/substate
execution. `resolve_sequence` owns source link fallback, clip ordering and exact
completion counts; runtime only resolves immutable DAT inputs. The C++ oracle
compiles original `get_link`, action/substate branches, zero-link completion, and
CSequence progression/hooks with the original intrusive list. It compares 2,400
steps of exact double cursor bits and hook order, and 27 chain cases. It does not
qualify GDLE root quaternion math: the oracle explicitly supplies no root frame.
The legacy half-open `PreparedMotion` helper remains distinct from this inclusive
physics executor. Same-substate updates preserve the live fractional cursor and change only cyclic
rate. A prepared StopCompletely suffix retains active links and FIFO completion
tokens, including zero-count Ready callbacks and source substate coalescing. Nine
rate cases and nine queue traces compile original GDLE list/manager operations.
Action completion before a multi-clip cyclic suffix remains unsupported.
Missing assets and those unsupported chains fail preparation.

The GDLE executor accepts explicit quanta through `f64::from(0.2_f32)`, matching
`PhysicsObj::UpdateObjectInternal(float)` through `CPartArray::Update(float)` into
CSequence's double cursor. Its oracle includes the 0.2 boundary. The current World
owner supplies a widened f32 1/30 step. GDLE's wall-clock catch-up/discard policy
(`PhysicsObj.cpp`, lines792–879) differs from the runtime's fixed-step scheduling
under stalls; these tests establish execution parity for admitted quanta, not
wall-time or lag parity. ACE player RootCycle has its separate <=0.1 step bound.

The CM_Style branch preserves the previous style's default substate when selecting
the destination cycle, direct/default-style link fallback, append order and
completion counts. `oracle/physical_style_generate.py --source /path/to/gdle`
compiles original get_link/style code; 36 vectors cover direct/fallback, missing
optional links/cycles, prior substates, reverse rates and cyclic warmup. Runtime
preparation records both sides of the style transition.

Prepared DAT chains retain explicit per-clip rate roles and raw rates. Positive
action-speed changes recompute Action/Current/Unit roles and cycle physics
separately, including the stop suffix; frame/hook arrays remain shared. Missing
metadata, sign changes and live cyclic-only retiming fail explicitly. Actual-DAT
runtime tests compare retimed actions with fresh source resolution; equal-rate
regressions ensure roles are never inferred from coincidentally equal floats.

Raw player locomotion now keeps forward, sidestep and turn hold keys separately.
The source interpreter preserves signed raw speeds (including negative TurnRight)
and applies the backwards factor only to WalkBackwards. The bounded adapter
accepts normalized axis strength in [-1, 1]; raw RunForward and arbitrary action
commands cannot grant extra movement or trusted attack hooks. Four hundred
thirty-two vectors compile the original pinned ACE `MotionInterp.adjust_motion`
and `apply_run_to_command`; independent per-axis overrides are regression-tested.

Cold style preparation now includes Ready, WalkForward (both rate signs), and
RunForward. Current-rate retiming preserves the source lookup sign and explicit
clip rate roles. Actual DAT tests compare fresh resolution with retiming, execute
Run/Walk→Magic transitions, and retain or clear side/turn modifiers according to
the authored cycle flags. Explicit style adoption resets forward to Ready without
consuming a client sequence; it uses the destination's prepared locomotion Arc.
