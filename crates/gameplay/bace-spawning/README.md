# bace-spawning

Owns pure generator selection, bounded immutable spawn work, occupancy and
lifecycle. World preparation, physics admission, inventory/vendor transactions
and durable placement belong to their named owners and return fenced receipts.
All time is explicit 30 Hz simulation ticks plus supplied Unix/day/event state.

`GeneratorMachine` preserves official ACE cumulative f32 threshold subtraction,
source profile order, unconditional -1 profiles, placeholder 3666 behavior,
initial batch/count quirks, 1001-attempt power-up bound, treasure clamping and
whole-profile occupancy, strict cooldowns, five-second staged status checks,
zero-timestamp initial delay, container/event activation, notification remapping,
reset/destruction directives and link profile cloning. `generator_destination`
preserves destination flag precedence and requested quaternion/offset behavior.
`Defined` time and the `Wield` destination have no invented behavior: the former
is inactive logic and the latter falls through to default placement, as in ACE.
An explicit Death notification matches exactly the Death regeneration flag;
upstream has no normal death-notification call site. Vendor shop generation is
opt-in; rotation offsets default to the prepared definition's configured value.

`GeneratorSpawnIntent` freezes the generator incarnation/content revision,
profile, occurrence, placement and random event identity. Blocked acknowledgments
retain it unchanged, with no reroll. Keyed Domain::Generator streams isolate
selection/materialization purposes and each generator. Invalid content produces
an explicit terminal diagnostic. Initial permanent placement failures retain
suppressed occupancy rather than a dangling destroyed entity identity. Queues,
profiles, registries and output effects are bounded. Mutation errors retain the
previous state; late receipts cannot consume a replacement incarnation. Shop
receipts sum actual retained stock contributions for precise later withdrawal.

Timers, queues and memberships are transient and restart from accepted content.
Lifecycle effects must be admitted by the simulation owner before adopting a
cloned state transition; a Nothing directive intentionally retains its members.
`SpawnSchedule` remains the smaller existing bounded explicit-tick respawn queue,
with earliest-eligible selection and exact-generation acknowledgment.

Compatibility authority: official ACEmulator/ACE
`47edade3bd3f6044b676d4eb877c4965c7eda62b`,
`Source/ACE.Server/WorldObjects/WorldObject_Generators.cs` and
`Source/ACE.Server/Entity/GeneratorProfile.cs`. See `oracle/README.md` for the
compiled unchanged-method fixtures. `tests/generators.rs` covers bounded retry,
receipt validation, suppressed occupancy, cooldowns, status staging and cleanup;
`tests/placement.rs` covers independent transform fixtures and flag/link rules.
Source wall clocks, global random state and dangling destroyed registrations are
replaced with explicit clocks, isolated streams and suppression records. No
.NET PRNG sequence or untested geometry admission parity is claimed.
