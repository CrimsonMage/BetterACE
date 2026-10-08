# bace-persistence

Implemented: opaque content/snapshot contracts and a bounded, simulation-owned dirty-save coordinator. No database or runtime I/O.

`DirtySaves::mark_at` coalesces immutable snapshots with explicit monotonic simulation time; callers supply monotonic mutation revisions and the current acknowledged database version. `mark` is shorthand using the last supplied time. `due` orders by the first unsaved mutation time plus five seconds, with stable admission-order ties. Coalescing and failures never move a pending deadline. Successful completion of an older snapshot advances remaining dirty age only to the earliest mutation after that snapshot was dispatched. This prevents a continuously updated low ID from monopolizing service ahead of older pending objects. An acknowledgment advances only the durable CAS version and does not clear newer mutations. Confirmed failures retain dirty state.

`reserve` atomically freezes a participant set after preceding writes finish. It prevents routine batches and new mutations until `finish_reserved` (confirmed commit) or `cancel_reserved` (confirmed rollback). Proposed transaction state belongs to the caller and MUST NOT be published before commit. `finish_reserved` takes the newly committed database version in `expected_version`. The caller must finish/cancel the complete reserved set.

The configured capacity bounds tracked aggregates; each aggregate is limited to 17 MiB, including its envelope. `new` also caps retained latest bytes and outstanding routine-request bytes independently at 64 MiB; `with_byte_limit` configures that budget. In-flight metadata stores no redundant payload clone. `failed` requires the matching request revision/CAS version, so a late failure cannot release newer work. `finish_reserved` checks the byte budget before changing any state; a capacity failure retains the reservation and must be resolved before reporting local success. Clean entries can be forgotten explicitly. Five seconds is a scheduling cadence, not a crash-loss guarantee during outages. A selection contains at most 1,024 snapshots. If the oldest eligible object cannot fit remaining in-flight capacity, newer small objects do not backfill the gap. `bace-runtime::saves` provides the bounded fair write worker and dedicated PostgreSQL pool; application code still owns retry/reconciliation, status reporting and integration into real gameplay.


Typed contracts also identify mapped generations, character ownership leases/epochs and semantic offline XP events/receipts. These do not embed physics state or implement allegiance calculations. `bace-runtime::persistence_thread` owns a dedicated OS thread and current-thread async runtime; routine/offline lanes receive alternating service after bounded critical bursts.


`InventoryOperation` carries stable operation identity, frozen snapshots,
participant leases and ownership comparisons. Online and offline actors use the
same fencing rules. Relational housing/item ownership and frozen player/item/corpse/
house DTOs now have database integration evidence, including contested transfer,
rollback, uncertain-request replay, reconnect and stale offline writes after login.
These contracts do not themselves instantiate gameplay or clear dirty state.

`OwnedSaveBatch` is the routine, receipt-free save contract for an owned hierarchy.
Changed snapshots and lock-only ancestor participants are separate: a dirty item
can be saved without incrementing clean character/container versions. Every
character ancestor carries its current lease; no routine batch changes ownership.
The runtime sends these batches through the same reserved age/deadline routine
lane and preserves the original dirty timestamp on failure. CAS acknowledgments
must still be applied through dirty-revision checks; unknown commit outcomes must
be reconciled before retrying. `LoadedInventory` represents a complete bounded
hierarchy loaded under a Loading lease, never a partial inventory success.

`PlacementOperation` carries explicit world/contained/equipped/removed comparisons,
lock participants, character leases and storage-view fences. `HousingOperation`
adds an ownership/access-generation comparison to that same durable receipt.
`LocatedSnapshot` reloads a bounded world-cell inventory forest; housing maintenance
pages preserve offline owner leases. Runtime critical queues retain these exact
requests through backpressure or uncertain commits. Ordinary owned saves remain in
the separately reserved, age-ordered routine lane without per-save receipts.

`VendorStockOperation` combines a world-epoch-fenced placement request with an
unplaced vendor marker CAS and an exact durable vendor aggregate version fence.
The marker and changed item/player snapshots share one operation ID and receipt;
`StoredVendorStock` loads the frozen marker bytes separately from item trees.
`StoredVendorSource` reads one exact durable V5 vendor aggregate and world cell
for pre-Use source preparation. `StoredVendorStockForest` carries the complete
ordered item aggregates and actual placements with the marker for cold admission.
`StoredVendorState` combines both views under one PostgreSQL hierarchy lock so
a commerce commit cannot split the vendor aggregate and marker across reads.

`ConstructedCreaturePromotionOperation` names every fresh Creature/Cow root in
an epoch-fenced placement request. Its V5 companions carry original generator
identity, equipment order, death roster and enchantment-bearing item snapshots.
The database verifies their committed relational graph before acknowledging the
exact operation; runtime must retain the same request through uncertain writes.
Simulation must retain the proposal through an uncertain result and adopt only
the committed marker and item acknowledgments.

Clean admitted aggregates can be seeded with `register_clean`, without manufacturing
an acknowledgment. `mark_batch_at` validates a complete immutable capture before
coalescing any row. `finish_reserved_hierarchy` atomically adopts durable changed
and newly acquired rows, retires rows whose ownership left the service, and releases
unchanged rows with their original dirty age. It is a coordinator receipt-adoption
boundary; the caller supplies only confirmed database results.

`AccountBanOperation` is the exact staff/automatic-expiry account mutation
contract: a stable 16-byte operation ID, expected account revision, issuer,
start/expiry in Unix milliseconds and optional reason. `AccountBanVerdict`
distinguishes missing, disabled, allowed, active and expired accounts. An expired
record still needs a CAS-fenced `Expire` operation before login admission.
`AccountBanListEntry` supports bounded active-ban pages with resolved issuer
names. These typed contracts do not authorize staff commands or perform I/O.
