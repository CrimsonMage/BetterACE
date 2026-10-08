# Independent ACE spell factory oracle

Official ACEmulator/ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b`,
AGPL-3.0-only, copyright ACE contributors. `extract.py` copies the unchanged
`LootGenerationFactory_Spells.cs`, `LootGenerationFactory_Magic.cs`, pet/aetheria
methods, spell/cantrip tables, chance tables, treasure
roll helpers and full spell progression. Enum declarations are extracted verbatim.
`source.sha256` records complete original file hashes and extraction scope.

The harness supplies item/profile properties, default cantrip multipliers (1.0),
controlled random variates and explicit synthetic SpellTable metadata
(`Formula.Level = spell ID % 8 + 1`, `Power = spell ID % 401`,
`BaseMana = spell ID % 31 + 1`). This metadata is deliberately synthetic;
no DAT asset claim is made. Source spell selection, correlated RNG consumption,
progression, requirements and difficulty arithmetic execute in original C#.

Using Python 3 and .NET SDK 8.0.408:

```
python3 crates/gameplay/bace-loot/oracle/spells/extract.py
dotnet run --project crates/gameplay/bace-loot/oracle/spells/Oracle.csproj --configuration Release > crates/gameplay/bace-loot/tests/fixtures/ace_spells.tsv
dotnet run --project crates/gameplay/bace-loot/oracle/spells/Oracle.csproj --configuration Release -- magic > crates/gameplay/bace-loot/tests/fixtures/ace_treasure_magic.tsv
dotnet run --project crates/gameplay/bace-loot/oracle/spells/Oracle.csproj --configuration Release -- special > crates/gameplay/bace-loot/tests/fixtures/ace_treasure_special.tsv
cargo +stable test -p bace-loot --lib
```

512 rows cover sixteen item/weapon/armor classes, four tiers, four variates and
two loot quality modifiers. Rust compares exact spell lists and draw counts,
f32 difficulty and both existing level requirements, including legendary cantrips.
Another 512 rows check the complete AssignMagic pipeline: mana, mana rate,
spellcraft, skill limits and lore difficulty. Both sets preserve source insertion
order. Another 240 rows check pet ratings, workmanship and coalesced mana/aetheria
addon occurrence, type and levels, including disabled/scaled aetheria rates.
The harness does not call Rust or generate expectations from the port.
