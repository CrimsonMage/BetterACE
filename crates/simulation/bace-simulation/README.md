# bace-simulation

Status: foundation.

Bounded pure command queue and explicit fixed-step synthetic kernel. No I/O or wall clock in tick execution. Complete gameplay orchestration is deferred.

`Kernel` now owns registered `CharacterProgression` aggregates alongside the same
world/physics owner. Registration requires an existing world actor and unique
actor/account/session identities; failed registration returns the aggregate so
dirty state is not lost. Session IDs must encode a connection generation, not a
recycled legacy slot. `take_character` transfers owned state back to a lifecycle
adapter; it does not acknowledge a database save or authorize an early logout.

`Command::RaiseProgression` carries authenticated action correlation. Every step
rechecks the actor/account/session binding and serial action sequence before
calling the character domain. Sequence wrap uses the half-range rule; authenticated
domain rejections consume their sequence, while invalid ownership/replay does not.
This is an intentional hardening over pinned ACE's unchecked GameAction sequence.

Correlated `ProgressionOutcome` values enter a bounded, reusable outbox. When it
fills, the next progression command stays queued without consuming its sequence
or mutating character state. FIFO order is preserved and physics ticks continue.
The adapter must drain `take_progression_outcome` before submitting indefinitely;
`progression_backpressured`, pending-outcome and queued-command counts expose
pressure. Limits are explicit through `with_gameplay_limits`; `new` retains the
existing interface and uses the command capacity for outcomes, with at most 4,096
registered characters. No accepted pose/physics state is duplicated.

Integration tests cover identity replacement, stale requests, sequence wrap,
unchanged rejected state, registration limits and full-outbox backpressure.
Character expenditure itself has independent pinned ACE golden fixtures in
`bace-character`. A runtime UDP progression path and bounded result delivery are tested. Frozen save DTO composition,
complete lifecycle drains and actual stock-client gameplay remain separate work.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.
