# bace-world-events

Scheduled and content-controlled world events.

Status: foundation; complete subsystem compatibility is not claimed.

Implementation belongs in named modules. Crate roots remain declaration-only.

`Events` now implements bounded explicit-time state transitions from pinned
`ACE.Server/Managers/EventManager.cs`: Enabled/Disabled/Off/On, strict start/end
time comparisons, case-insensitive names with `@` suffix removal, and the special
read-only `EventIsPKWorld` query. Native NPC StartEvent/StopEvent/InqEvent call
this owner rather than fabricating an emitted-effect success. Full event-driven
generator activation and durable world-event restart composition remain separate.
