# bace-db-postgres

Implemented: SQLx PostgreSQL pool, migration, binary candidate publication journal, accepted heads, rejection diagnostics, opaque entity snapshots, CAS writes and idempotent atomic multi-object operations. SQL and connections are confined to this adapter.

Native SQL inserts into `content_candidates(wcid,class_name,weenie_type,payload)` automatically join a publication identified by their database transaction. A trigger obtains a commit-held revision-row lock, records the durable publication and sends a notification. Revisions therefore cannot be missed by an allocation-order/commit-order race. Candidate rows cannot be updated or deleted. A publication is bounded to 100,000 candidates / 256 MiB, polling responses to 256 MiB, and snapshot write batches to 1,024 objects / 64 MiB. PostgreSQL `xid8` requires PostgreSQL 13 or newer.

Poll `pending_publications(0, limit)` on startup/reconnect. The caller must decode and validate the complete candidate catalog and matching scalar identities before `accept_validated`; rejected candidates use `reject` and later independent publications continue. Accepted heads survive rejection/restart. No JSON is stored. `active_content` loads accepted heads in one statement. Runtime catalog preparation and bounded tick delivery are implemented separately in `bace-runtime::publication`; a notification listener, tombstones and publication latency monitoring are not implemented here.

`save_batch` commits complete aggregate snapshots atomically with CAS versions. `valuable` additionally records a durable operation ID and internally generated SHA-256 request fingerprint in the same transaction. Reusing an ID for another request fails. Repeating the exact request after commit returns `AlreadyCommitted`; `resolve_operation` retrieves the committed fingerprint. Participants must remain reserved until the outcome is known. `CommitUncertain` explicitly requires resolution before retrying. An absent operation while the original transaction may still be in progress is not proof of rollback: repeat only the same operation ID and request. Routine uncertain commits must be reconciled by loading the object version and bytes while overlapping writes remain blocked.

The `AccountRepository` implementation persists fresh Argon2id PHC records supplied by `bace-auth`, atomically enforces canonical-name uniqueness and always creates Player/non-disabled accounts. It never verifies passwords in the SQL adapter. Names use exact canonical-string uniqueness with C collation; invalid persisted hashes/access levels fail explicitly. No account migration or first-account automatic promotion is implemented.

This adapter does not implement trade gameplay, network session policy or simulation. Relational inventory ownership and frozen identity checks are implemented below; runtime owns bounded asynchronous save workers.

Run `cargo test -p bace-db-postgres`. The integration suite requires `initdb` and `pg_ctl` on PATH and a non-root user. It creates an isolated temporary PostgreSQL cluster with a private Unix socket and no TCP listener; it never connects to existing services. Missing prerequisites fail explicitly rather than silently skipping the suite.


Additional foundations: mapped manifest metadata with parent-CAS acceptance (`accept_mapped`, `active_generation`), paged candidate reads, character ownership epochs/login/logout fences, fenced single-character saves and idempotent semantic offline XP receipts. Files must be validated and durable before mapped acceptance; the SQL adapter performs no filesystem I/O. Offline XP currently updates a capped cache only: full allegiance propagation, online consumption and session integration are not implemented. See `docs/persistence.md` for limits and ownership contracts.


## Gameplay durability expansion

Migrations 0005–0008 add relational item/container slots, account-owned player
identity, housing ownership, and persistent player/dynamic ID allocation. World
content stays in aggregate disk `.bace` packs: PostgreSQL stores accepted manifest
metadata, not a copy of the imported world tables.

`create_player` atomically reserves account/name/slot and saves a frozen character
with all starting items. `inventory_operation` checks every affected character's
online or offline epoch, source ownership, aggregate CAS and bounded ancestry;
slots are unique and cycles reject the complete transaction. Duplicate operation
IDs return their original receipt; changed requests reject. Existing generic
writers reject owned items/containers/houses, preventing a fencing bypass.

`save_offline` shares login's ownership lock, so either the offline update commits
before load or its stale lease rejects. `save_house` saves frozen permissions/rent
state under the online/offline owner's fence and an idempotent operation receipt.
Housing purchase/transfer policy and game UI remain separate gameplay work.
`WorldOwner` holds a dedicated connection lock; explicit startup recovery advances
old leases in bounded batches while preserving durable snapshots. Lifecycle must
stop admission if that connection is lost; this API alone is not a running host.

`initialize_local_database` provisions/restarts only a marked private Unix-socket
cluster, disables TCP and migrates its BetterACE database. Existing unmarked
directories are refused. This optional helper is Unix-only; externally configured
PostgreSQL remains the portable deployment interface.

`load_character_inventory` traverses nested containers under the current Loading
lease and the hierarchy sequencer. Count, depth and total payload limits are
checked from indexed scalar metadata before fetching blobs; every returned item
retains its relational location and durable version. It returns no partial tree.
`save_owned_batch` writes existing dirty aggregates with sorted ancestor locks,
complete character leases and atomic CAS checks, while preserving relational
player/house identity. Clean ancestors participate without redundant writes.
These routine writes do not grow the durable operation journal; uncertain results
still require reconciliation. Generic save APIs remain barred from owned items,
characters and housing. Critical transfers keep their existing durable receipts.


`load_character_inventory` holds the Loading fence and hierarchy sequencer while
checking bounded scalar counts/depth/lengths before fetching nested snapshots.
`save_owned_batch` writes only dirty aggregates with CAS while locking clean
ancestors and their leases. It creates no per-routine-save operation receipts.
A shared final gate prevents player/account/name or house/owner identity changes
inside critical, routine, offline, online or logout payloads. Legitimate identity
changes require an explicit corresponding relational operation.

Migration 0011 adds incremental native profile candidates/heads. Namespace 46/47
SQL inserts use the same immutable, transaction-ordered journal as weenie edits.
`pending_native_publication` returns one complete bounded batch or a capacity
error; `accept_mapped` advances both head families atomically with the manifest.
Legacy catalog readers/acceptance reject batches containing native profiles, so a
mixed SQL transaction cannot silently publish only its weenies. The mapped runtime
worker accepts bounded mixed weenie/profile batches as one generation. Migration
0021 adds source-row and ClothingBase candidates plus explicit tombstones to the
same journal. Reviewed inbox batches are fenced to the accepted manifest they
were compared against; stale previews cannot overwrite an intervening generation.

Explicit placement transactions now atomically compare world/contained/equipped/
removed locations with frozen snapshots, quantity changes, owner leases and durable
operation receipts. Separate main/backpack/equipment slots preserve client ordering.
World-cell tree reload includes nested bags and corpses with bounded scalar-size
preflight. Removing a nonempty container fails; corpse identity/death receipts are
preserved instead of decoded as ordinary item state.
Native PVE death placement transactions also carry a separate frozen kind-104
receipt. It is unplaced, tied to the accepted event ID and fenced world epoch,
and gives zero-drop NoCorpse deaths an exact durable write. The database rejects
marker ID reuse, placement changes for the marker, and attempts to write it
outside its matching world placement operation. Region restoration never treats
the receipt as a world item or generic creature.

Housing lifecycle transactions change the nullable owner and access generation in
the same commit as player/payment snapshots. Abandonment/eviction retain contents;
unowned or stale storage views fail, while the next owner inherits contents.
Storage views bind actor identity; owner-account or storage-guest permission is
checked under the same locks. Bounded maintenance pages expose offline leases and
frozen houses without creating physical actors. Complete static house-link and
allegiance permission admission still belongs to the authoritative runtime view.

Registered gameplay records cannot be overwritten by older schemas or a different
aggregate kind. Existing rare identities/key versions cannot be erased or changed,
and attempt/timer ordinals and effective time cannot move backward. Key fingerprints
are immutable versioned public metadata; secret master keys never enter PostgreSQL.
Checks read bounded prior data and preserve snapshots/receipts on rejection.
Item schema 5's optional source CreateList destination is checked under the same
item CAS: a known origin cannot be changed or erased, and a legacy unknown origin
cannot be invented on migration. Placed V5 items load through the world-tree
reader; live item acquisition/writers must still adopt the schema before player
NoCorpse can use that origin.

Frozen schema3 item, corpse and house registries survive world-tree reload,
character inventory transfers and routine owned writes. The
`tests/postgres/enchantments.rs` suite exercises real PostgreSQL identity/lease
fences, exact idempotent transfer receipts, and rejection of older schema writers
without erasing stored timers or metadata. House timer revisions are produced by
the simulation owner; the database never advances enchantment clocks.

Migration 0015 adds account revision CAS and the staff account-operation journal.
Account creation/access/password changes share one transaction with their durable
operation receipt. Fingerprints contain the salted password verifier, never the
plaintext password; receipts expose no verifier. Exact replay returns the original
result after later account changes. Independent PostgreSQL tests cover stale CAS,
operation mismatch, rollback, reopen/replay and ordinary auto-created Player access.

Migration 0016 adds nullable account creation seconds for source public-chat age
gates. Existing rows remain unknown; subsequent native and staff account inserts
receive the database's real transaction timestamp. `account_creation_time` is a
bounded authenticated-ID lookup and never synthesizes an age for historical rows.
The isolated PostgreSQL regression verifies migration, unknown history, new
account timestamps and malformed/missing identities.

`lookup_player_identity` uses the existing canonical-name or player-ID index;
`lookup_player_identities` resolves at most 1,024 distinct saved-friend IDs in one
query. Results expose character ID, account ID and display name only, never
credentials or account names. Missing rows are explicit `None`/omitted batch
members. Runtime must keep pending action/session order until read completion.

Migration 0017 retains ACE's signed Int32 TotalLogins beside character ownership.
Only a successful fenced Loading-to-Online transition increments it. The exact
Online lease resolves its committed `OnlineLoginReceipt` after uncertain completion;
retries and aborted loads never increment again. Native pre-migration rows start a
new count history at zero because earlier aggregates had no login counter. Signed
counter exhaustion rejects before the Online transition, a documented hardening
against source overflow. Real PostgreSQL tests cover races, abort/reconnect, restart,
stale receipt reads and exhaustion; the runtime projects the source ushort cast.

`staff_gag_operations` (migration 0020) journals exact player gag mutations. Both
online and offline writes share the character-ownership lease lock with login and
logout and compare snapshot versions. Offline edits additionally compare the full
frozen aggregate after removing only IsGagged/GagTimestamp/GagDuration and the
expected revision increment. Exact replay resolves before checking a newer lease;
changed bytes under an existing operation ID are rejected. No account credentials
or raw command text enter this journal.

Migration 0022 stores an unplaced vendor stock marker under a unique
vendor-to-marker identity. `vendor_stock_operation` fingerprints the exact marker,
vendor version, item/player placement request and world epoch before committing
them in one transaction. It checks ordered stock references against contained
ItemSaveV5 trees, source identity and marker CAS; a failed tree or stale vendor
rolls back every row and the operation receipt. Exact replay returns the prior
durable outcome. A vendor must already have a durable aggregate at the expected
version before lazy stock can be admitted; transient-only vendors remain held.
`load_vendor_stock_forest` holds the inventory hierarchy sequencer, preflights
the 64 MiB aggregate bound and verifies every V5 tree against its relational
placement before returning a complete ordered cold snapshot.
`load_vendor_state` reads the V5 world vendor and optional ordered stock forest
under that same lock in one transaction. A purchase cannot commit between its
source version and marker reads; separate source/forest loaders remain available
for narrower callers. A real PostgreSQL default-Buy regression commits player
V6 CoinValue, coin V5 debit, fresh V5 grant, vendor counters and marker V2 in
one receipt, then cold-loads their matched state. It also checks stale-CAS
rollback and exact replay. This storage proof does not establish live Buy output.

Fresh V5 constructed Creature/Cow roots enter through
`constructed_creature_promotion`, an exact world-epoch placement operation with
explicit ordered root identities. The transaction rejects missing V5 descendants,
incorrect equipped membership and unrelated death-roster IDs before its item
rows or durable operation receipt can commit. Ordinary placement rejects that
fresh V5 construction shape; V4 historical fixtures and mature item updates
remain readable. Cold restoration and simulation owner admission occur above
this adapter. BetterACE retains the documented deviation of preserving durable
generated equipment identities instead of ACE cold NPC equipment rerolls.
The relational graph verifies direct equipped membership and every death-roster
parent. It does not encode the source's equipment pass ordinal; the frozen V5
companion retains that order, and exact operation replay fingerprints its bytes.

Migration 0023 retains pinned ACE account-ban start, expiry, issuer and reason
beside the account's CAS revision. `apply_account_ban` journals the exact
Ban/Unban/Expire request and result in the same transaction; stale revisions,
early expiry and malformed reasons leave both account and journal unchanged.
`account_ban_verdict` gives a bounded login read with explicit missing, disabled,
active and expired states. Login must commit an `Expire` request before treating
an expired ban as cleared. `list_active_account_bans` pages active bans by
canonical name and resolves current issuer names without loading credentials.
The pinned ACE source is `SentinelCommands.HandleBanAccount/HandleUnBanAccount/
HandleBanlist`, `AuthenticationHandler.HandleLoginRequest`, and
`AuthenticationDatabase.GetListofBannedAccounts` at the repository pin. The
database API is a durable boundary; staff policy, online boot and auth output
remain with their application owners.
