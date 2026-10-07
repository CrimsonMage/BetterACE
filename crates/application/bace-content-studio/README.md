# bace-content-studio

Owns BetterACE Content Studio: an egui native weenie editor, legacy import/export
and offline pack-building interface. `bace-content-gui` is its thin launcher.
Conversion semantics and pinned ACE provenance remain owned by `bace-import`;
native validation/export remains owned by `bace-content-tools`.

The editor supports new/open/save/save-as, clone-as-new, bounded undo/redo,
validation and a TOML source view. Its forms cover 21 property families: scalar
properties, attributes/vitals/skills, body parts, spells, create lists, structured
emotes/actions, books/pages, generators, positions and visual overrides.
Named property choices derive from official ACE's pinned enum definitions;
unknown numeric IDs stay editable. Optional fields remain absent until added.
Integers use exact text parsing, including i64 limits. Ordered rows can move;
legacy SQL sequence IDs can be explicitly renumbered to match the list.

One document is open at a time. Source files are capped at 16 MiB; undo history
is capped at 32 snapshots / 32 MiB. Saves validate before writing, use a flushed
temporary file and atomic replacement, detect changed-on-disk files against the
loaded hash, and preserve dirty state on failure. This check is not an OS-wide
concurrent-writer lock: external writers must not race a save after the hash
check. Save As never overwrites an unrelated existing file. New/open/close
protect unsaved changes. File I/O runs on a bounded background job.

Legacy export creates a new bundle with `native.toml`, a JSON or SQL copy, and
explicit conversion notes. Authoring metadata without legacy counterparts stays
in the complete native companion. SQL regenerates relational row IDs and uses
destination defaults for absent timestamps; it emits INSERTs, not destructive
replacement statements. An authored sequence conflicting with SQL ordering is
rejected until explicitly renumbered. No database is contacted by export.

Weenie-Fab is a workflow reference at commit
`2b2d514077209fc49b645f9467b9e0c418799b1a`, not a compatibility authority; its
source/assets were not copied. See `docs/content-studio-coverage.toml` for the
feature comparison. EmoteScript compilation/decompilation, death-treasure loot
profiles and automatic gameplay-derived calculators remain unsupported;
structured emote values and spell IDs are editable, with searchable pinned
spell enum names. Persistent editor preferences remain unsupported.

Select or drop up to 128 files, choose an existing output folder, and convert.
Each successful source gets a fresh subfolder containing one TOML per weenie
and a source-hash manifest. Existing files are never overwritten. A failed or
cancelled source removes its partial output; other completed sources remain.
Cancellation takes effect between sources/records, after any active MariaDB
operation finishes its bounded cleanup. Closing waits for the active job.

JSON accepts the existing ACE and Lifestoned/GDLE weenie dialects, one object per
file (16 MiB limit). SQL uses the existing isolated MariaDB staging importer
(512 MiB source limit); select the private installation directory containing
`bin/mariadb`, `bin/mariadbd`, and `bin/mariadb-install-db`. Full-world tables
outside weenie conversion are unsupported and reject the source. SQL staging
is currently enabled on Linux only; macOS/Windows SQL setup is unvalidated.
The desktop/JSON implementation targets all three platforms; validation on
each platform must be reported separately, not inferred from portability.

One background worker processes files sequentially; a capacity-one result
channel bounds pending messages. The source queue and results are capped at
128 files, diagnostics at 8 KiB. JSON is bounded before parsing. SQL retains the
existing importer's bounded staging/extraction costs. Output serialization is
one template at a time. GUI dependencies are isolated from server/CLI builds;
eframe uses OpenGL without wgpu, and idle frames are event-driven.

The Build tab compiles native TOML folders into immutable `.bace` files plus a
binary generation manifest. Compilation retains only one decoded source and a
compact ID/offset index, with binary payloads spooled to disk. Builds accept at
most 100,000 sources / 8 GiB of payloads; directory traversal is bounded to
200,000 entries and 16 levels, and rejects symlinks. Conversion `manifest.toml`
files are skipped. Duplicate IDs reject the whole build. Output appears in a
fresh folder and incomplete builds are cleaned up. This does not publish to
PostgreSQL or activate packs on a server. The runtime content-worker still has
an in-memory catalog; bounded gameplay cache/pack integration remains incomplete.
See `docs/pack-format.md`.

Launch a built `bace-content-gui` executable directly; during development use
`cargo run -p bace-content-gui`. Linux file dialogs need an XDG desktop portal
backend or Zenity. Upstream UI APIs: <https://docs.rs/eframe/0.36.2/eframe/>
and <https://docs.rs/rfd/0.17.2/rfd/>.

## Validation

Validated on Linux: workspace clippy, tests and architecture checks using the
documented `+stable` toolchain fallback. Formatting passes for all four changed
crates. A full formatting sweep passed earlier; the final sweep reports concurrent
unrelated edits in `bace-session/src/registry.rs` and `tests/registry.rs`.
The explicit Studio SQL test
converted the unchanged official Arrow fixture and rejected unsupported SQL;
all three `bace-import` MariaDB integration tests also passed with the private
installation. JSON tests cover authored values, non-overwriting repeated
exports, malformed/unknown/oversized input and cancellation before work.

The built native window was exercised under Xvfb with software OpenGL and
Zenity file dialogs: select the synthetic Lifestoned JSON fixture, select an
output folder, convert, and verify the resulting TOML and manifest. Windows,
macOS and the XDG portal dialog path were not validated. Cancellation during
an active SQL subprocess and crash durability are not established by this
smoke test. The expanded editor was also exercised through native JSON opening,
exact i64 display and TOML Save As. The Build tab produced a `.bace` file and
binary manifest through the actual folder-picker/button flow. Separate tests
reopen compiled packs with mmap and decode individual records. Editor tests cover
undo/redo, duplicate-ID rejection, external file-change protection, metadata
companions and section prototypes. The explicit SQL exporter integration passed
with the complex fixture and real MariaDB. No server/playability or complete-world
compatibility claim is made.
