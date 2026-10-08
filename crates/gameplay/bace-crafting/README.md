# bace-crafting

Recipes, salvage and crafting mutations.

Pure crafting and salvage proposal domain. The simulation owner reserves the
participants and the persistence adapter commits the complete proposal before
reporting success. These APIs do not independently mutate an inventory or send
packets.

Implementation belongs in named modules. Crate roots remain declaration-only.

`tinker_chance` preserves official ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b`
`RecipeManager.GetTinkerChance` material modifiers, ten attempt multipliers,
intermediate float arithmetic, effective skill and logistic probability. Explicit
owner policy caps ordinary imbues at exactly 33%, or 38% with one 5 percentage
point augmentation; low skills retain their lower calculated probability.
Foolproof tools retain 100%. `oracle/generate.py` executes the pinned C# methods
with synthetic adapters; 770 vectors cover all material IDs and ten attempts.
The policy caps are separately tested, not falsely attributed to ACE.

`quote_craft` freezes the actor, recipe and both items. `propose_craft` rejects a
stale/expired quote, ownership/trade/reservation changes and failed requirements.
Typed property mutations and success/failure consumption produce before/after
snapshots only. Positive requirement predicates must be normalized from ACE's
inverted failure predicates when preparing recipes. The operation's stable ID and
character random identity scope independent success/source/target draws through
`bace-random`. Repeating an operation returns its original random outcome; only a
durable operation receipt establishes whether it has already committed.
Unknown recipe scripts and result creation must be rejected during preparation,
not interpreted as successful no-ops. Reviewed script and spellbook support is
described below.

`propose_salvage` follows pinned GDLE
`353cbab52ef7da2b7063bc3e3f008461d8531693`, `Source/Player.cpp`, for 195-divisor
double-precision yield, 0–4 augmentations, best-skill selection and material
aggregation. 135 independent compiled C++ vectors freeze that scalar formula.
Stacked inputs are explicitly unsuitable. Bounded proposals retain unsuitable
IDs, split all units into at most 300 bags of 100, and preflight capacity/templates
before consumption. They preserve total unit counts and value below GDLE's
explicit 75,000 per-material cap (`value_capped` reports that cap).
Each split carries the aggregate workmanship ratio; numerator/count are ratio
metadata replicated across splits, not conserved original-item counters.
Callers must emit one result per message and an empty response when no result
exists, including unsuitable IDs once, implementing observations #30–32.

Hardening tests cover stale confirmations, mutation overflow, ownership, trade,
duplicate input, missing templates, full inventories and retry stability.
The deterministic statistical test checks each of 32 characters over 10,000
attempts in four scenarios, with a predeclared two-sided Hoeffding/union bound
and family-wise alpha 1e-6. It does not establish retail RNG equivalence.

Source attribution: official ACEmulator/ACE contributors and GDLE contributors,
AGPL-3.0-only. Source hashes and pins are embedded in the synthetic fixtures.

Salvage reply metadata follows GDLE's request-wide skill selection: if any item
uses the better tinkering yield, all bag reports identify that tinkering skill
and report no salvage augmentation. Otherwise they identify Salvaging and its
augmentation bonus. Every generated bag has exactly one matching result, including
100-unit splits. The replication/runtime bridge emits these only after durable
receipt adoption; no-result operations receive one empty acknowledgment.

`prepare_native_recipe` joins bounded imported recipe rows in ACE participant,
property-family and modification order. It retains source messages and normalizes
failure predicates exactly once. `oracle/recipes/modifications.py` executes the
original `RecipeManager.VerifyRequirement` and `Modify*` methods: 1,149 comparison
vectors and 768 modification vectors cover missing properties, source GUID
copies, result-index fallback, null string removal and AddSpell. Spellbook
membership is an explicit property kind; runtime persists actual spellbook rows.

All 44 active embedded recipe scripts are implemented, including conditional
thresholds, integer truncation, DID copies, retained flags, imbues and paragon
properties. `oracle/recipes/generate.py` compiles the unchanged original
`MutationCache`, `EffectArgument` and `Effect` classes and embeds the original
scripts. Its 393 vectors compare exact floating-point bits and missing-property
behavior. A script's Int171 update changes the structured tinker count and log
once; imported recipes never also enable the legacy manual increment.
Overflow and malformed/oversized input reject the complete candidate atomically.
This checked-overflow behavior is an explicit hardening deviation from unchecked
source arithmetic.

The imported converter currently rejects result creation, vital deltas and
copy-to-result operations because their aggregate owners are not integrated.
Native preparation is not evidence of a generic recipe chance implementation:
the current proposal API uses the reviewed tinkering/imbuing chance policy.
Actor property mutations also require explicit simulation-owner adoption before
live admission. Unsupported content must be diagnosed before a confirmation is
issued; it must not consume items or report success.

`select_new_tinkering_recipe` implements the pinned `RecipeManager_New.GetNewRecipe`
tinkering fallback after an exact cookbook miss. The original C# switch cases and
`SourceToRecipe` table produce 2,328 independent vectors in
`oracle/recipes/selection.py`; generic dye, society and result-producing recipes
are outside this entry point. `oracle/recipes/messages.py` executes the original
`BroadcastTinkering`, `GetNameWithMaterial` and `FloatExtensions.Round` methods.
Its fixtures cover inscription text, material-name replacement, stripping the bag
quantity suffix, failed attempts, chance midpoint rounding and Single custom
formatting. Verified DAT material names must be supplied by the adapter.

`propose_confirmed_craft` allows unrelated actor UI/age changes while preserving
all quoted chance inputs, actor properties read by requirements or mutations,
recipe, item snapshots, random identity and original expiry. Changed relevant
inputs reject before any draw. Source/target revisions remain exact. The original
strict proposal API remains available. Successful simulation adoption moves
surviving modified source then target to slot zero, matching ACE `UpdateObj`; the
same inventory proposal reserves and saves shifted siblings.
