# Reviewed content inbox

The authenticated BetterACE host console can import native TOML while the host
process is running. Set `pack_directory` to the directory holding the accepted
`.bace` generation and `content_inbox_directory` to a writable staging folder.
PostgreSQL must be reachable through `database_url_env`. The inbox is scanned
**only** when an operator clicks **Review inbox**. It never downloads ACE data or
imports files just because they appear in the folder.
Grant the assisting team write access to this staging folder through the host
operating system; only the host operator needs dashboard credentials.

For this checkout, the ignored `.local/server.toml` points at
`.local/content-inbox` and `.local/world-packs`. The local PostgreSQL database
has the mapped-content migration applied. From the repository root, load the
ignored `.local/server.env` into the shell and start
`bace-server host --config .local/server.toml`. If the host operator has not
been provisioned, run `bace-cli host-init --state-directory state/host` first;
it reads a password from standard input and refuses to overwrite an operator.
Use the printed loopback dashboard URL. No ACE import is run by these steps.

Place complete native TOML documents directly in these folders:

| Folder | Content |
|---|---|
| `weenies/` | Weenie templates, including generators and their properties |
| `world/` | Typed world rows: landblock instances and links, encounters, quests, recipes and their child rows, spells, treasure and other imported tables |
| `clothing/` | Native `ClothingPatchV1` ClothingBase overrides |
| `loot/` | Native loot graphs |
| `rares/` | Native rare profiles |

For example, `world/new-quest.toml` uses the same TOML emitted by
`bace-content-tools::export_world_record`. The file name is for the team; the
record's typed namespace and ID determine its pack key. Every file replaces one
complete record. Unrelated keys, including local custom entries, are retained.
Landblock and parent-link lookup indexes are derived from the affected world rows
inside the same delta; teams edit the source rows, not the indexes directly.
An identical record is skipped. A changed record appears as **REPLACE** in the
review. A new key appears as **ADD**.

To remove a record, choose its type and ID in the dashboard's **Stage removal**
form. This writes a small native TOML file under `removals/`; then **Review
inbox** shows the removal before confirmation. Teams can also drop a
`removals/*.toml` file with `kind` and `id`, or use the inbox root's
`remove.toml` for a batch:

```toml
[[remove]]
kind = "weenie"
id = 12345

[[remove]]
kind = "quest"
id = 678
```

`kind` accepts `weenie`, `clothing`, `loot`, `rare`, or a world table name such as
`landblock_instance`, `encounter`, `quest`, `recipe`, or `recipe_mods_int`.
Review rejects a missing key, duplicate key, invalid TOML, bad identity, or an
oversized batch. It checks the currently accepted mapped pack and displays every
add, replacement and removal. The dashboard then asks for confirmation. A
publish request rechecks the source files and accepted manifest; if either
changed, review again. A batch is limited to 4,096 records and 16 MiB of source
and compiled payload. Split larger work into reviewed batches.

Confirmation commits one immutable candidate revision to PostgreSQL. The game
child's pack worker validates the complete revision, writes an immutable delta `.bace`, and
atomically changes the accepted manifest. At most two deltas remain active before
compaction. A rejected revision leaves the last accepted generation intact.
Weenie additions, replacements and removals require an accepted base with the
namespace 48 and 49 name indexes; a legacy base must be upgraded first.
The dashboard shows accepted, pending and rejected revision counts; details
appear in host diagnostics. It follows the queued revision until acceptance or
rejection and displays any rejection reason. Accepted additions and replacements
become unchanged files; accepted removals become harmless tombstoned files.
Archive staged files when convenient to keep the folder clear.

The dashboard queues content even if the game child is still starting. That
revision remains pending until the child can validate and publish it. The game
runtime adopts accepted generations for future region inputs; already active
regions and readers retain their pinned immutable generation. The crafting
catalog reloads accepted recipe rows on the next crafting request after adoption;
in-flight requests keep their original revision. Gameplay consumption of mapped
quest rows and ClothingBase overrides is still integration work. A queued revision is not an
active game change until the accepted revision advances and the running world
adopts it. Nonpermanent regions cool after 30 seconds of inactivity and enter
save-before-unload at five minutes; removal of an already active object is not
instantaneous.
The loopback host console can be reached remotely through an authenticated SSH
tunnel; host credentials remain separate from game accounts.

The ACE treasure table set is not a live inbox type. New world builds include
pinned table set ID 1. To upgrade an existing compacted accepted world, stop the
game child gracefully, rebuild from identical logical world sources, and use
`world-activate --reindex` so the new base supplies namespace 52; then restart
the child. Reindex requires every existing logical source record to be present
unchanged in the rebuilt base; a world with additional native source rows must
include those rows in the rebuild. Game startup fails if the selected table set
is missing or corrupt.
Once a table set is accepted, reindex also preserves its bytes; changing a
profile needs a separate candidate publication path and child restart.
