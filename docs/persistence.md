# Persistence implementation notes

The current storage foundation exposes opaque binary payloads. The content/codec owner validates envelopes and decoded scalar identities; the PostgreSQL owner persists bytes and constraints. Binary format bytes are never legacy network packets.

## Publication

A transaction inserting one or more `content_candidates` creates one durable pending publication. Trigger allocation serializes through `content_clock` until commit, rather than relying on sequence allocation order. Immutable candidate rows remain available after newer imports. Processing decisions are serialized independently and must occur in revision order. Rejecting the earliest invalid publication removes it from pending work, preserves accepted heads, and lets later unrelated work proceed. Corrections insert new candidates. Database operators can inspect `content_publications.status/rejection`.

`bace-runtime::publication` builds the replacement catalog in `spawn_blocking`, verifies decoded identities against scalar columns, reserves capacity in a bounded delivery channel, persists acceptance, then sends an immutable snapshot for the world owner to install at a tick boundary. A crash between acceptance and runtime swap is recovered by reading accepted heads. On startup/reconnect, polling from zero is safe because decided rows are excluded. A cursor must never advance past an unprocessed pending publication. Notifications are hints; no listener is required for correctness.

The content-worker harness selects shutdown during connection, startup loading and each complete publication attempt, including channel backpressure and database acceptance. Its pool applies server-side statement/lock/idle-transaction deadlines; client operations, consumer shutdown and pool closure have explicit timeouts. Cancellation exits the worker instead of resuming an in-memory catalog after a potentially committed acceptance. A restarted worker reloads durable accepted heads and pending work. Already-running blocking validation cannot be forcibly canceled; the command's Tokio runtime has a bounded shutdown wait. The real PostgreSQL test holds locks during both startup and acceptance, requests shutdown while each lock remains held, and verifies recovery on restart.

`content_status` reads exact scalar counts and the accepted generation from one statement over journal metadata and heads. It does not load binary candidates or cap the pending count to a polling page. The restricted-role regression grants no candidate-table access and still obtains correct status, including more than 100 pending batches. Counting journal metadata still scales with journal history; this is not a constant-time query claim.

## Saving

Mutation revisions identify simulation snapshots; database versions count committed writes. Coalescing multiple mutations can therefore advance the former many times while the latter advances once. CAS predicates always use the database version. Older save completion updates that version but leaves newer mutations dirty.

The caller owns one bounded write sequencer. Before a valuable operation, reserve all touched aggregates and wait for previous writes; snapshot the proposed complete states, atomically commit with a durable operation ID, then acknowledge and release the reserved aggregate set. SQL applies stable object-ID lock order and rolls back every participant on any CAS conflict. The stored operation fingerprint covers sorted participants, expected versions, mutation revisions and payload bytes. Exact duplicates do not execute again; changed requests under the same ID fail.

Confirmed failures retain dirty state. Unknown commit results require durable reconciliation; they are not permission to release reservations or roll back local state blindly. Routine state uses a five-second scheduling cadence, but prolonged database failure can exceed five seconds of unsaved state. The dedicated runtime writer described below is implemented; a gameplay transaction layer and persistence health monitor still require application integration.

## Verification

The real PostgreSQL integration test covers fresh/repeated migration, direct SQL publication, candidate immutability, invalid publication quarantine, recovery of accepted heads, prevention of stale snapshot overwrite, atomic rollback of partially processed multi-object writes, durable operation retry, changed-request rejection, and serialization of competing candidate transactions. Dirty-coordinator tests cover coalescing during an in-flight save, bounded capacity, stale acknowledgments and reservation/rollback sequencing.

## Fresh accounts

`PgStore` implements the SQL-free `bace-auth::AccountRepository` contract. Account names arrive canonicalized by the auth domain; PostgreSQL C-collated uniqueness compares those exact strings. Concurrent creates have one winner, and duplicate creation never overwrites credentials. New accounts always receive Player access and disabled=false. PHC records contain their own salts and bounded Argon2id parameters; the adapter validates their representation on lookup and performs no memory-hard work. The real PostgreSQL account test hashes/verifies on blocking workers and covers duplicate/racing creates, mixed-case lookup, privilege mapping, disabled state, and corrupt persisted credentials.

The dirty coordinator now independently bounds retained latest-state bytes and in-flight request bytes (64 MiB each by default). Failure completion is keyed by the submitted mutation/CAS revision, preventing a late old failure from releasing newer work. In-flight entries retain metadata only. Committed reservation completion checks all limits before mutation; capacity failure preserves the reservation, so application recovery must create room and complete local installation before releasing success.

## Save scheduling without starvation

The single simulation owner uses `DirtySaves::mark_at` and forwards each batch's earliest `dirty_since` to the runtime writer. Coalescing preserves the first unsaved deadline. Failure preserves it too. After an acknowledgment covers an older snapshot, the next deadline starts at the earliest mutation not covered by that acknowledgment; hot objects cannot retain ancient priority after their ancient state has become durable. Eligible snapshots sort by deadline and stable admission order, not object ID. A batch is bounded to 1,024 records. Older large snapshots reserve the next available capacity by preventing younger small snapshots from repeatedly filling the remaining bytes.

`bace-runtime::saves::spawn_postgres_save_worker` opens its own one-connection write pool. Routine and valuable traffic have separate bounded queues and separate byte permits, so valuable traffic cannot exhaust the routine allocation. Admission per scheduling turn is capped; the worker checks routine deadlines before each single storage operation and yields afterward. After at most four valuable operations by default, an eligible routine request runs. Routine requests are ordered by original deadline, then stable admission order. This provides eventual service for admitted eligible saves under healthy bounded storage and the single-owner admission protocol, even under continuous valuable/new-save traffic. Queue-full refusal borrows and returns control without consuming the owner's state; the owner retains the same age when retrying.

The worker accepts already-coalesced batches; it does not perform coalescing or invent dirty timestamps. If no valuable request competes, it may flush a submitted routine batch early. The usual caller submits from `DirtySaves::due`, which owns the five-second schedule. Dropping all handles drains admitted work and closes the owned pool. Dropping a completion receiver does not cancel an admitted write.

Each operation has a bounded future timeout. The reserved PostgreSQL pool additionally installs server `statement_timeout`, `lock_timeout`, and idle-transaction deadlines so a canceled Rust future cannot strand that one connection indefinitely. Lock timeout is shorter than statement timeout, which is shorter than the operation timeout. A blocked-row integration test keeps the lock held while an unrelated following save succeeds on the same pool.

Failures and uncertain timeouts return explicit tickets carrying the original dirty age and scheduling lateness. They do not count as successful saves or clear owner state. A failed ticket transfers retry/reconciliation responsibility back to the dirty owner; this worker does not silently retry or discard dirty state. Uncertain commits must be resolved before releasing reservations or retrying. During a database outage no scheduler can guarantee persistence by a deadline; retained dirty data and surfaced failure/lateness are required. The runnable game loop and persistence health reporting remain separate integration work.

## Dedicated persistence thread and offline foundation

`PersistenceThread::postgres` now constructs the reserved writer pool on a named OS thread with its own current-thread Tokio runtime. Its generic factory is also executed there. The existing async-only save factory remains available for harnesses; production composition should use the dedicated thread. `SaveHandle::close` requests admission closure for every clone, then admitted requests drain. `PersistenceThread::drain` closes admission and joins after completion; database operation deadlines bound individual attempts, not guaranteed successful durability. Owners retain dirty snapshots and consume failure tickets. The thread does not manufacture success during an outage.

The third offline lane has independent count/byte capacity. After a bounded critical burst, due routine and offline requests rotate, preserving oldest-first ordering within each lane. Offline tickets retain their original queue age. An offline request first durably admits its semantic event, then applies it under an offline ownership lease. A transition race can therefore return failure with an unapplied event still available for recovery.

Character ownership is a persisted `Offline -> Loading -> Online -> LoggingOut -> Offline` fence. Login increments the epoch and reads the aggregate in the same transaction while excluding offline mutation. Logout increments the epoch before the final CAS snapshot, so delayed online saves are rejected. Finishing logout commits the final bytes and releases offline ownership atomically. Aborting a load advances the epoch again. `character_lease` supports reconciliation after uncertain commits; automatic restart takeover and session orchestration are not implemented. The caller must include effective logout skill values in its still-to-be-defined character DTO.

Registered characters reject unfenced `save_batch` and `valuable` writes. `save_owned` and `try_owned_routine` provide single-character fenced saves. Multi-character owned valuable operations remain a future gameplay integration contract, rather than bypassing ownership through the generic snapshot API.

`OfflineXpEvent` receipts compare immutable event ID/source/target/amount independently of character CAS. Duplicate admission is harmless; changed semantics under the same ID fail. Applying an event atomically updates the capped offline XP cache and records its result. Repeating it does not credit twice. This is an offline cache foundation only: full allegiance calculations, atomic source-XP-plus-event admission, online credit routing, cache consumption into total/unassigned XP, deletion handling, global pending-work discovery and receipt retention are not composed yet. Pending records can be recovered per character without loading every player. No offline physics is created or advanced.

## Mapped generation metadata

`MappedGeneration` stores manifest SHA-256, optional parent hash, base hash, accepted publication revision and bounded binary manifest bytes. `accept_mapped` assumes the caller has validated and durably installed files; it performs no filesystem operations. It atomically checks the active parent, accepts ordered candidate heads and advances the database's active generation. Exact committed retries verify all metadata. Layout-only initialization/compaction keeps the accepted revision unchanged. Once a mapped generation is active, the old decoded-catalog acceptance path refuses further acceptance. Rejection continues to preserve the last accepted generation.

`pending_revision` and bounded `candidate_page` let a compiler process records incrementally; use a one-record page to keep candidate payload memory bounded by one record (17 MiB). The old full-catalog API remains only for transitional harnesses and must not be used as the new runtime catalog. Filesystem durability, mapped views and manifest decoding belong to the pack compiler/runtime owners.


## Frozen gameplay saves and ownership

Storage kinds 100/101/102/103, schema 1, are respectively PlayerSaveV1,
EntitySaveV1 items, CorpseSaveV1 and HouseSaveV1. Their frozen nested WeenieV1
property representation preserves unknown numeric IDs and absent/zero values;
character metadata carries non-property hair textures, option masks, titles and
spell favorites. Quest and house access collections are bounded during decoding.
Unrecognized schema versions fail explicitly; future changes require migration.

Account/player names and slots, character epochs, item/container slots and housing
owner identities remain relational. Creation with initial possessions and valuable
inventory/house changes commit atomically with stable operation receipts. Online
and offline writes share ownership fencing; a login cannot race an old offline
snapshot into the database. Routine acknowledgments still use mutation revision
and database CAS separately and retain newer dirty state.

World templates and source tables are stored in aggregate `.bace` disk packs.
PostgreSQL records only which immutable manifest is accepted, alongside the existing
publication journal. The complete world is not copied into PostgreSQL content rows
by the bootstrap workflow. This division is independent of player/item/housing
save storage, which remains in PostgreSQL.

## Social, allegiance and durable travel/death checkpoints

Player schema 5 wraps frozen schema 4 and adds schema-1 social preferences;
explicit migrations retain all earlier player supplements. Allegiance nodes and
metadata use storage kinds 115 and 116, schema 1, with relational character,
account, patron, monarch and chat-room identity constraints. Row versions are
independent of player versions; runtime restoration uses the maximum stored
mutation revision and verifies the complete forest.

Allegiance writes and player XP snapshots share a receipt journal. The composite
allegiance-placement operation extends that same transaction to corpse/item
placement and, when present, the NPC continuation checkpoint. A failed CAS leaves
all rows and the receipt absent. Reusing an operation ID with changed semantics
fails; exact retries return the existing durable result. All changed characters
require current leases, including offline ancestors. A cold forest read bounds
row count and combined payload bytes before fetching payloads in one repeatable
snapshot.

Travel checkpoints include the final destination and resource debits. Death
checkpoints include final respawn state, corpse expiry/owner and item changes.
An interrupted presentation sequence restores the committed living destination;
item loss is never replayed separately. Ambiguous outcomes retain reservations
and the exact frozen request until resolved. Receipt adapters reject mismatched
acknowledgments and cannot reinterpret a later retry failure as proof that the
original operation did not commit.
