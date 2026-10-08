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
protect unsaved changes. Content and DAT I/O runs on bounded background jobs; the small local preference file is read/written synchronously.

Legacy export creates a new bundle with `native.toml`, a JSON or SQL copy, and
explicit conversion notes. Authoring metadata without legacy counterparts stays
in the complete native companion. SQL regenerates relational row IDs and uses
destination defaults for absent timestamps; it emits INSERTs, not destructive
replacement statements. An authored sequence conflicting with SQL ordering is
rejected until explicitly renumbered. No database is contacted by export.

Weenie-Fab is a workflow reference at commit
`2b2d514077209fc49b645f9467b9e0c418799b1a`, not a compatibility authority; its
source/assets were not copied. See `docs/content-studio-coverage.toml` for the
feature comparison. Vitaeum and vitaeum4's local inspectors informed the viewport
workflow only; no proprietary source or assets were copied.

The EmoteScript tab opens/exports `.es`, generates source from native emotes,
and explicitly compiles a draft into the document with undo. Compilation is
native Rust, capped at 1 MiB / 10,000 lines/actions / 64 indentation levels.
It supports named and positional fields, nested branch links, exact i64 values,
ranges and movement/heading shorthand. Errors return no partial document.
Compile/apply replaces emote rows and regenerates legacy relational IDs; native
TOML is the authoritative save. Full upstream grammar/decompiler parity and
external weenie-name lookup are not claimed; use explicit fields/numeric IDs.
Unapplied script drafts block native save/export and are protected on close.

Loot profiles have a separate `DeathTreasureV1` TOML editor with JSON import/export
and SQL export. Validation precedes writing; exports never overwrite a file.
This is offline authoring: the weenie pack builder does not accept loot profiles,
and runtime loot interpretation/publication remains separate work. Legacy JSON
metadata outside the supported profile fields rejects import rather than disappearing.

The authoring helpers explicitly apply skill/vital initial levels using formulas
from the selected DAT, bulk body armor, and monster spell roll chances. They
exclude player gear, buffs and augmentations. Effective spell chances assume the
displayed spell order; they are not a full runtime gameplay simulation.

## Model preview and asset lookup

Choose `client_portal.dat` in **3D model & DIDs**. The selected path is remembered
in a local TOML preference file under the platform's application configuration
directory; assets are never auto-discovered or copied. **Inspect DIDs** loads the
index/references without requiring valid model geometry. **Load preview** resolves
a Setup or GfxObj; **From weenie** also reads the explicit model, texture and
palette overrides. **Apply appearance to weenie** updates Setup, ClothingBase, Palette,
PaletteTemplate and Shade together with undo. The candidate is validated before
any document change; palette-set DIDs cannot be stored as a Palette property.

The bounded Rust preview resolves ClothingBase setup/model/texture replacements,
palette sets and ranges, texture surfaces, DXT1/3/5 and common direct/indexed
pixel formats. Drag to orbit, right-drag to pan, scroll to zoom. DID lookup lists
Setups, ClothingBases, palettes, palette sets and GfxObjs; the inspector shows
palette swatches and resolved references. Unsupported/malformed assets return
visible errors. Preview uses static placement frames, a depth buffer, flat lighting
and alpha testing. The offline workspace can display one selected animation
frame with `ReplaceObject` hooks. Animation playback, particles, translucent
blending, full client shader fidelity and automatic character-creation
appearance composition are unsupported.
These assets are for inspection, not authoritative world collision.

Only selected assets are decoded, with 50,000 triangles, 64 MiB expanded texture
pixels, 16 MiB palette patches and 32 million raster samples per frame as limits.
One worker/result and one displayed scene bound pending preview work. The archive
index is read per load, then the archive is closed. The UI retains selected scene
assets, not the complete DAT. Rendering is cached until camera/scene changes.

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

## Offline workspace and sandbox

**Offline workspace** opens an existing `.manifest` and its immutable `.bace`
segments read-only. Choose a content type and browse its records in bounded
pages or enter an ID, edit selected source records through field forms or TOML, duplicate one
under a new ID, and save native drafts into `weenies/`, `world/`, `clothing/`,
`loot/`, `rares/` or `animations/`. `removals/*.toml` and `remove.toml` stage
explicit deletions. Saves validate the native codec and check for an external
file change before atomically replacing a draft; the opened pack is never
modified. One unsaved draft is guarded on record switching and window close.

The **Recipe editor** panel accepts a recipe ID and finds its cookbook uses,
requirements, success/failure effects and typed effect details in the opened
generation plus saved drafts. Selecting any part opens its validated field form;
each part remains a separate native record and saves separately. The lookup is
explicit and runs on a background worker. It does not simulate crafting or
show unsaved in-memory form changes until those drafts are saved.

**Review changes** compares the draft folder against the selected generation,
skips byte-identical records, shows additions/replacements/removals, and
prepares the affected landblock, instance-link, creature-name and class-name
indexes. A separate confirmation is required when the active three-segment
limit requires candidate compaction. **Build candidate .bace** rechecks the
review fingerprint, copies the original immutable segments into a fresh output
folder, writes the delta and manifest there, and reopens the result. It never
accepts a server generation or contacts PostgreSQL. Existing server inbox
review can publish supported native TOML types; animation-swap namespace 51
is currently offline only and is rejected by live publication.

**World sandbox** reads user-selected Portal/Cell DATs on a worker, displays one
actual landblock height mesh plus bounded static/pack location markers, and
supports landblock/coordinate teleport and WASD/QE flying. Terrain colors and
markers are inspection aids: static meshes, dungeon interiors, collision and
stock-client shaders are not yet represented. The selected pack generation is
read-only and records are decoded on demand. **Visual part-swap preview**
renders a selected Setup and forward animation frame with the draft
ClothingBase or animation `ReplaceObject` overlay. **Emote table test** runs the
native deterministic emote manager against disposable local inventory/quest/XP
state. It traces supported effects and stops explicitly on effects or queries
that need live world or durable services; motion requests do not yet drive the
world sandbox.

Supplied Linux DAT smoke tests loaded landblock `A260` as 128 triangles and
rendered a compatible human Setup animation frame with an inserted
`ReplaceObject` hook. These checks use local assets only and do not establish
stock-client visual fidelity.
The ignored `supplied_world_pack_builds_one_quest_delta` test uses
`BACE_WORLD_MANIFEST` to verify a real 16PY quest replacement leaves the
original pack bytes intact and produces a separately reopenable candidate.

Launch a built `bace-content-gui` executable directly; during development use
`cargo run -p bace-content-gui`. Linux file dialogs need an XDG desktop portal
backend or Zenity. Upstream UI APIs: <https://docs.rs/eframe/0.36.2/eframe/>
and <https://docs.rs/rfd/0.17.2/rfd/>.

## Validation

Automated coverage includes safe saves/undo, exact integer editing, SQL/JSON
conversion, mapped pack reopen, malformed EmoteScript, branch links, loot schema
validation and malformed/indexed/compressed textures. Official pinned C# decoders
produce synthetic visual/vital golden fixtures in `bace-compat`; C# is a development
oracle only, never a Studio runtime dependency.

Linux native smoke tests use Xvfb/software OpenGL and Zenity file dialogs. Supplied
DAT tests resolve human Setup `02000001` (417 triangles / 40 textures) and five
ClothingBase/template combinations. No proprietary asset bytes are committed.
Run explicitly with `BACE_DAT_DIRECTORY=/your/DAT/folder cargo test -p
bace-content-studio supplied_ -- --ignored --nocapture`. Ignored prerequisite
tests are not passes. Windows, macOS and the XDG portal dialog path still require
validation. These checks do not establish full client appearance or gameplay parity.

The expanded Linux UI smoke run selected a DAT through the picker, rendered the
human Setup and ClothingBase `10000001` with palette swatches, opened the upstream
cow EmoteScript example, compiled it, and saved native TOML containing the expected
seven sets / 17 actions. The selected DAT path persisted in an isolated test config. The native close
guard retained a modified loot profile until explicit discard.
Workspace tests and `xtask check` passed during this work; final focused tests and
Studio Clippy passed. Workspace formatting/Clippy also ran and reported concurrent
unrelated edits (runtime/gameplay/asset modules); this is not a clean global lint
claim. The real MariaDB import/export prerequisite suite passed separately.

## Custom ClothingBases

Open **ClothingBases** in the workspace rail, then **Open TOML / mod JSON**.
The structured editor handles multiple setup variants, ordered body-part model
changes, multiple texture swaps per part, palette templates/icons, mixed direct
palettes and palette sets, and multiple ranges per effect. DIDs are hexadecimal;
range offsets/counts are color counts in multiples of eight (zero count denotes
2,048 colors). **Duplicate setup** retains all its changes for editing another
setup ID. **Duplicate template** retains every palette source/range and chooses
a free template ID. **Undo / redo** keeps one bounded previous document snapshot.

**Preview & DAT lookup** resolves the current draft against the original selected
Portal DAT, including ClothingBase IDs absent from that DAT. Existing setup and
palette-template keys are replaced wholesale; other base entries remain. Changes
are never layered over a previously resolved draft. **Copy inspected table into
document** makes a complete editable copy of a loaded table. Setup/template
selectors list resolved variants after inspection, including inherited DAT entries.
**Copy selected setup/template into draft** copies one complete entry with undo,
preserving the destination ClothingBase DID and unrelated entries. New custom
IDs appear in the Clothing lookup list; **Copy DID** supports transfer to fields.
Changed preview controls show a reload notice until the displayed scene refreshes. Each explicit reload
validates the complete custom reference set before rendering. A valid open override
also applies in the weenie 3D view when its ClothingBase DID matches.

The compact **File** menu contains New, Save As, Export mod JSON, Close and
Discard actions. **Save TOML** uses atomic replacement and external-change detection for an opened
native file; Save As/JSON exports require a new filename. JSON imports become
unsaved native documents. A separate close guard protects ClothingBase edits and
unapplied source drafts. JSON export preserves multiple changes and palette order
in the mod's original shape. No DAT is modified. The separate offline workspace
can compile ClothingBase patches into a candidate `.bace` generation. Live
visual application and equipped outfit layering are not implemented.

The three examples from OptimShi/CustomClothingBase commit
`122145d0f6d0c159183f0f229aae611085a67bf1` passed explicit supplied-DAT rendering:
Celdon direct-palette recolor (419 triangles), green Virindi Mask (376), and the
new Rynthid table (1,112). The last example's filename is `10000900.json`, but its
actual `Id` is `10000903`; import uses the document ID. Missing preferred texture
images fall back to an available image in that texture's own mip list, with a
visible reference report. Missing all images remains an error. Full client shader
or live-server mod parity is not claimed. Reproduce with `BACE_DAT_DIRECTORY` and
`BACE_CUSTOM_CLOTHING_REFERENCE` set, then `cargo test -p bace-content-studio
supplied_custom -- --ignored --nocapture`. Reference data stays outside Git.

The ClothingBase Linux GUI smoke imported the Celdon mod example, saved native
TOML, exported legacy JSON, and rendered its selected palette. Synthetic regression
tests cover selective-copy preservation, atomic appearance application, rejection
of invalid palette/template/shade values, and one-step undo/redo.

The variant-selector GUI check loaded inherited setup `0200004E`, copied its
model replacement and both texture swaps into the current draft, and undid that
change. Workspace tests, workspace Clippy and the architecture check passed after
these additions. The final workspace formatting check also passes.
