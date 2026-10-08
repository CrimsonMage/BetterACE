# bace-interactions

Doors, locks, switches, portals, lifestones and books.

Status: foundation; complete interaction gameplay is not implemented.

Implementation belongs in named modules. Crate roots remain declaration-only.

`DoorAuthority` now implements GDLE-led door control from
`353cbab52ef7da2b7063bc3e3f008461d8531693`: nonautonomous opening/closing motion,
server/action counters, action-busy admission, captured initial lock state,
authoritative-use admission and collision-triggered NPC opening. Commanded state,
interpreted motion and collision solidity are separate. The asset-provided
ethereal hook tests authoritative occupancy; an occupied closing door stays
ethereal and retries on each physics update. Inputs must come from the simulation
owner, never client-reported contact or pose.

Sources: GDLE `Source/Door.cpp`, `Source/Monster.cpp` (`DoCollision`),
`Source/PhatSDK/PhysicsObj.cpp` (`set_ethereal`, `UpdateObjectInternal`) and
`Source/PhatSDK/AnimHooks.cpp` (`EtherealHook::Execute`). AGPL-3.0-only, GDLE
contributors. `oracle/gdle_door.py` verifies the pinned checkout and compiles
verbatim `set_ethereal` against synthetic collision/cell adapters;
`tests/fixtures/gdle-solidity.csv` covers all 16 loaded-cell overlap/ethereality
combinations. These are state transitions, not packet or full door-geometry
qualification. Generation-fenced stale hooks, fail-closed missing geometry and
restoration of initially open content are explicit BetterACE hardening beyond
the GDLE implementation's initially-closed assumption. Divergences #17/#28 require
physical integration validation; no new packet structure is inferred here.

`bace-simulation/tests/doors.rs` now connects authenticated Use requests and
prepared animation hooks to synthetic dynamic colliders on the same world owner.
Real swept NPC contacts can trigger unlocked opening; client-reported contact
cannot. Three integration scenarios cover occupancy holds, real collider movement
blocking and full-outbox request/sequence retention. Source packet output and
world DAT admission remain separate qualification boundaries. Do not project
`is_closed()` directly into solidity or bypass unavailable geometry.

World policy, recall/binding and player-death scalar rules derive from the pinned
ACE WorldManager/Landblock/Player source. `oracle/ace_world_policy` compiles unchanged
source extracts and freezes exact zone lists, vitae floors, item-loss counts, PK
classification and recall movement boundaries. `world_policy` consumes those
fixtures rather than duplicating the Rust calculation. Death possession selection
has its own compiled-source oracle and bounded immutable output; simulation owns
random identity, reservations, delayed presentation and exact durable adoption.
