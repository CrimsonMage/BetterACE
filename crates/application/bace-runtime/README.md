# bace-runtime

Status: foundation.

Thin application composition plus working content publication and fair dedicated save workers. `serve` refuses readiness; complete stock-client gameplay/world orchestration is not implemented.

`simulation::SimulationWorker` moves the kernel onto one named OS thread; physics and world share that owner. Bounded nonblocking command admission, bounded work per tick, explicit shutdown/join, 30 Hz pacing and fixed-size timing histograms keep adapter work separate. `exercise` uses this worker unpaced. A rejected command is currently counted, not routed to a stock-client correction packet. Worker shutdown does not itself integrate the save coordinator: durable drain composition remains part of the playable milestone.

The content harness uses two asynchronous runtime workers. Saves use a separately reserved PostgreSQL writer connection with bounded operation deadlines; they do not run on the simulation thread.

Simulation submission acknowledges queue admission only. Closure rejects new submissions. The legacy report field `discarded_commands` counts unapplied adapter inputs; recoverable shutdown returns those inputs with the kernel. Calling `wait` on an unlimited worker returns an error after stopping and joining it, so the stop handle cannot be lost in an endless wait. Tests cover queue-full ownership, continuous input, finite/explicit shutdown accounting and a blocked save adapter while ticks progress.

See root ARCHITECTURE.md and AGENTS.md for MUST rules.


Networking foundation now includes a dedicated `NetworkThread` owning the paired
UDP adapters, bounded peer drivers, generation-fenced sessions, account replacement
and explicit owner-drain acknowledgments. The API has real loopback handshake and
reliable-message tests. `try_send` acknowledges queue admission only; callers must
handle `CommandRejected` and `Terminated`, and retain ownership until drain. Invalid
packets do not refresh liveness. A byte-identical challenge is retained for bounded
retries with matching credential identity, intentionally differing from ACE's
remove-on-repeated-login behavior. No reliable traffic is emitted before cookie proof.

`AuthenticationPool` has a fixed worker count, bounded work/completion capacity and
blocking Argon2 execution. Auto-creation is enabled by default through AccountConfig.
Graceful drain returns completed and unresolved session identities, including worker
panic information, and waits for blocking password jobs even after request timeout.
Session generations fence all asynchronous outcomes.

`DatPreparationWorker` owns archive reads and compression on separate bounded
blocking capacity. Rejected jobs retain caller ownership; preparation results are
fenced against the DDD generation. Explicit shutdown recovers accepted results and
unrecovered jobs. Dropping the worker counts abandoned results, never DDD success.
Fingerprint validation/catalog production must occur before composition.

These APIs are integration building blocks. `serve` still refuses readiness:
complete character persistence, world/asset admission, gameplay routing and
production session/DDD composition are not complete. Network shutdown does not
claim that simulation or persistence has drained. Linux loopback tests are not
Windows/macOS or stock-client qualification. Network idle polling currently sleeps
1 ms and processes bounded rotating batches; shard capacity is not performance-qualified.


The simulation adapter now has a bounded correlated progression-result channel.
A stalled result consumer retains the overflow result and backpressures commands;
physics keeps ticking. `Kernel::try_enqueue` returns rejected command ownership,
so adapter admission cannot silently lose a command when the kernel is full.
`shutdown_recover` / `wait_recover` return the single kernel owner, unapplied inputs,
undelivered channel results and any tick failure. Kernel outbox results remain in
the returned kernel. Report-only exit refuses to discard characters or progression
results and returns `WorkerError::RecoveryRequired` carrying that state. Invalid
startup configuration or OS thread creation failure also returns kernel ownership.
These transfers are not persistence acknowledgments; Drop is not a graceful save drain.

`character_assets::prepare_character_assets` turns verified decoded XP/skill/CharGen
inputs into shared domain tables and heritage allocation rules, retaining all original
data. It does no I/O and supplies no synthetic fallback. Tests prepared the pinned
user-supplied portal archive (38 skills, 13 heritages); complete character lifecycle,
appearance instantiation and durable saves remain pending.

`PortalClock` separates the explicitly supplied world-time origin from monotonic
reliability deadlines and wraps the packet header's 16-bit time field correctly.
The stock-client time range from pinned DerethDateTime is checked; invalid origins
or exhaustion fail explicitly. Zero remains the synthetic harness default. Real
world composition must supply its authoritative portal-year origin; client echoes
cannot change it. This does not implement `@settime` or claim that the reported
other-shard time-change crash is resolved.
