# bace-replication

Status: foundation. Bounded per-object sequence counters and per-observer knowledge
are implemented; complete authoritative object descriptions, visibility queries,
movement scheduling and complete world-to-wire projection remain unsupported.

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
simulation owner → primary projection → reliable UDP. Rank-up sounds/chat, derived
vital/run-rate effects and durable character saves are still separate work. A
primary packet batch does not assert that the complete ACE action handler or save
transaction has been implemented.
