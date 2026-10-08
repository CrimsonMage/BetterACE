# Generator item spell routing oracle

Official ACEmulator/ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b`,
AGPL-3.0-only. `extract.py` copies the unchanged `CreateItemSpell` school
switch from `Creature_Magic.cs`, `Spell.HasItemCategory` and the complete
`SpellCategory` and `MagicSchool` enums. The small harness only supplies handlers and records which
object was targeted. `source.sha256` records exact upstream source files.

Reproduce with Python and .NET SDK 8:

```sh
python3 extract.py /path/to/pinned/ACE
dotnet run --project Oracle.csproj --configuration Release > ../../tests/fixtures/ace_generator_item_spell_routes.tsv
```

The 40 vectors cover every magic school, six legacy item aura categories and
ordinary item categories. The simulation tests separately cover the source
`TryWieldObject` 0.1-second delay, atomic permanent registry application,
item caster identity, reservations, cancellation and capacity backpressure.

Only Enchantment meta effects are prepared here. Unsupported boost, transfer,
projectile, portal, dispel, fellowship and periodic/proc equipment shapes are
explicit preparation errors. This bridge retains and activates equipment rows in
the existing magic owner; general NPC attribute/skill/weapon/armor aura evaluation
in the physical combat profile remains a pre-existing combat limitation. Registry
activation does not claim that every stat modifier affects NPC physical combat.
