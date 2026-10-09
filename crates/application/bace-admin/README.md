# bace-admin

Administrative command handling.

Status: foundation. Embedded local BetterACE dashboard, separate host credentials,
bounded browser sessions, CSRF/origin/host checks and SSE diagnostics. Native
Windows private-ACL provisioning remains unsupported. See docs/host-console.md.
The host dashboard also offers explicit [content inbox review](../../../docs/content-inbox.md):
an authenticated, CSRF-protected preview and confirmation request routed to
`bace-runtime`; this crate does not parse TOML or write packs.

Implementation belongs in named modules. Crate roots remain declaration-only.

The pinned official command catalog now retains all 324 attributes (321 unique
names), access levels, flags, raw-argument behavior and minimum arity. The parser
bounds input and rejects unmatched quotes; valid parsing and metadata have an
independent original-C# constructor/parser oracle. `knownobjs` selects the real
Developer handler instead of the empty Admin duplicate; `nudge` selects the later
content handler; `deaf` remains source-unimplemented. Reflection ordering is not a
portable upstream guarantee. Catalog presence is not a claim that every owner
handler is implemented. Typed `StaffOperation` conversion and simulation/runtime
adapters determine executable command coverage; unknown conversions return None.
Secrets are redacted in command and credential diagnostics.

The pinned Sentinel `HandleBoot` is typed for character, account and prefixed
instance-ID lookup. It joins the name after the selector and separates the
optional reason at the first comma. The runtime resolves only accepted online
sessions; source TODO commands and incompatible backend commands stay excluded.

Pinned Sentinel `@ban`, `@unban` and `@banlist` are typed without claiming that
catalog metadata executes them. `@ban` retains duration strings because ACE
looks up the account before reporting an invalid day, hour or minute. Runtime
owns the durable operation and source response/Audit ordering. The original
handler and `AuthenticationDatabase.GetListofBannedAccounts` at the baseline
pin provide the output provenance; parser tests check the three typed routes.

The separately authenticated chat pull API serves only accepted General, Trade
and Audit events. Per-bot scopes cannot grant host control or game privileges.
Defaults retain 4096 events and 8 MiB per channel; opaque channel/epoch cursors
report gaps from retention, dropped publications and restart. Publication uses a
bounded nonblocking queue. Both HTTP requests and connections have fixed budgets.
Credential TOML uses private-file checks and contains generated 256-bit bearer
tokens; missing credentials fail enabled startup. Runtime owns worker supervision
and shutdown. This is a live feed, not durable chat history or Discord posting.
Audit publication now reserves bounded queue capacity before assigning its feed
sequence. A full queue leaves the accepted event with the runtime social owner
for exact retry; General and Trade still report visible dropped publications.

`prepare_shard_command` prepares the five official `AdminShardCommands` handlers
for the runtime shard owner. It retains ACE's five-UTF-16-unit interval prefix,
unsigned negative-zero and trailing-NUL parsing, shutdown message joining and
world open/close/boot selection. Preparation does not confer authority: runtime
rechecks the live principal, including sudo account access. The original complete
C# handler file and unchanged manager methods are compiled by
`oracle/run_shard.py`; 110 command cases, 153 countdown windows and nine date
vectors are consumed by the runtime shard tests. The harness stubs only clocks,
threads, logging and network delivery; source SHA-256 hashes are in the fixture.

The five broadcast aliases (`gamecast`, `gamecastlocal`, `gamecastemote`,
`gamecastlocalemote`, `we`) now enter a typed, reauthorized simulation action.
Both local aliases intentionally use the global online audience, as the pinned
ACE handlers do. Emotes replace literal `\\n`; ordinary broadcasts retain the
source sender prefix. The source oracle runs all five unchanged handlers; fifteen
player-session cases are compared to the owner output, with separate wire and
backpressure tests. Console source vectors are recorded but do not qualify a
composed host-console route. Other catalog entries still require their concrete
owners; catalog coverage is not execution coverage.

Command catalog membership is not execution coverage. Authorization distinguishes
source TODOs from deliberately incompatible ACE implementations and native-owner
candidates. The incompatible handlers are rejected before target resolution or
mutation dispatch: managed-runtime collection/status (including composite
`allstats`, which invokes source `GCStatus` and database queue diagnostics), ACE mutable cache management,
legacy live SQL/JSON import and export, synthetic Biota database benchmarking, and
historical ACE-schema repair commands. Native adaptations and still-unwired
compatible candidates are labelled separately in live command help. In particular,
`createliveops`, `databasequeueinfo`, and `verify-*` are not blanket exclusions.

`@myloc` parses to a dedicated Developer self-position inspection, separate from
the selected-target `@targetloc` route. Its simulation owner emits the three
private Broadcast lines in pinned `DeveloperCommands.HandleMyLoc`; parser and
owner tests cover the distinction. This does not qualify `@targetloc`'s full
source presentation. Selected `@targetloc` now emits the four pinned source
lines from accepted world pose. Its optional explicit-GUID form remains
unsupported until current-landblock and global-object resolution are owned;
it does not silently inspect the selected object.

`@whoami` parses to a separate Developer self-identity inspection. It requires
an entered world binding and retains the pinned `ObjectGuid.ToString()` X8
format, distinct from Envoy `@myiid`'s unpadded hexadecimal source line.

`@listplayers` retains the pinned Developer command's optional first access
filter for the runtime online-session owner. A catalog entry or typed parse
alone does not establish an online roster or a host Console route.

`@gps` parses to a distinct Developer self-position inspection, separate from
`@myloc` and selected-target `@targetloc`. The simulation owner supplies its
one-line output from accepted world state.
