# bace-gameplay-api

Typed commands, effects and subsystem contracts.

Status: foundation; no complete gameplay or protocol compatibility claim.

The first typed boundary carries authenticated action correlation and progression
requests/results/read projections. `ActionContext` is server-created correlation,
not authorization by itself: simulation must check the live session/account/actor
binding before mutation. `ActionResult` is a domain result, never an automatic
durable success or packet. Wire serialization stays in network crates.

Attribute and maximum-vital IDs preserve official ACE numeric identities;
untrusted u32 values are checked without truncation. Skill IDs resolve against
authoritative character state. No request exposes an accepted-pose setter.

`CreationAllocation` carries six named attribute values and the exact 55 skill
advancement choices. Its Coordination/Quickness order follows CharacterCreateInfo,
not the reversed PropertyAttribute ID order. It intentionally represents only
allocation validation, not complete character creation or name reservation.

`CharacterBinding` identifies the authoritative session/account/actor association.

`StaffAction::Audit` is an internal post-commit staff action, not a client
message. Simulation rechecks Sentinel authority and routes accepted Audit text
to the social owner. An outcome acknowledges owner admission, not downstream
network or external feed delivery.

`StaffInspection::WhoAmI` retains the distinct pinned Developer source output
under the existing authenticated, read-only staff inspection contract.
`StaffInspection::Gps` likewise names the pinned one-line Developer location
inspection; simulation, rather than command text, supplies accepted pose.
`ProgressionOutcome` distinguishes stale sequencing or identity failures from
domain expenditure rejection; its original `ActionContext` remains available
for routing without client-selected authority. Simulation supplies the bounded
outbox and owns mutable progression state.

`ProgressionProjection` now includes optional typed `TraitDetails` and the
authoritative advancement class. None means missing input, not a request to
synthesize zeros. Detail fields remain domain state; per-recipient sequence
counters and packet layouts belong to replication/wire. `TrainSkill` retains
the client's quoted credits as an untrusted signed integer. Training outcomes
carry updated trait snapshots, actual remaining credits and the dirty revision;
they are not durable acknowledgments. Server-only specialization has no client
request type granting permission to bypass device/quest eligibility.

`ProgressionChange.follow_up_vital` carries one optional immutable authoritative
vital view for a ranked Endurance attribute. Simulation fills it from the
accepted character and World current after maximum refresh; replication owns
its counter and output order. A missing view remains an output obligation.

Full character creation/load/save ownership and the remaining gameplay command/effect
families are not implemented. Add each typed family with its owning subsystem and
source-backed behavior; do not bypass unavailable systems with successful no-ops.

Implementation belongs in named modules. Crate roots remain declaration-only.

`StaffAction::Heal.target_name` is a bounded copy of the accepted visibility
blueprint's display name made by the runtime owner. It is output text for the
pinned ACE nonplayer rejection only; simulation still checks current staff
authority, target presence, landblock scope and player identity before mutation.

`ProgressionActionRejection::DurabilityPending` is an explicit pre-authorization
hold: it consumes no action sequence and permits retrying the identical request.
Other progression rejections are terminal attempts. Simulation returns this bounded
outcome instead of stopping its command queue ahead of the receipt that releases
the reservation. The inventory/XP queue regression covers that deadlock and the
subsequent valid same-sequence attempt; malformed or replayed actions still reject.
