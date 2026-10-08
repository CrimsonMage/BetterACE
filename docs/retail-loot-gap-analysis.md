# Retail container observations versus pinned ACE treasure tables

The project owner supplied `loot-tier-analysis.zip` (seven JSON summaries, dated
2026-05-13) and identified its source as retail-era PCAP data from external
containers, possibly including chests. The archive contains neither item-level
records nor source-container IDs, extractor code, capture coverage, or a tier
definition. This report treats it as observational evidence, not as an ACE
treasure-profile oracle or a set of drop probabilities.

## What the archive shows

The tier summary counts 320,555 observed items. Its categories and item counts
sum within each tier. The apparent tier boundaries track wield difficulty:

| ZIP tier | Items | Share | Wield difficulty | Largest categories |
|---|---:|---:|---|---|
| 1 | 173,454 | 54.11% | 0 (the histogram's exact zero count) | armor 37.9%, clothing 19.1%, jewelry 15.7% |
| 2 | 691 | 0.22% | 1–30 | clothing 88.3%, armor 11.3% |
| 3 | 26,816 | 8.37% | 60–150 | armor 43.5%, clothing 30.9%, jewelry 21.2% |
| 4 | 29,951 | 9.34% | 160–200 | armor 51.1%, clothing 23.3%, jewelry 22.4% |
| 5 | 5,939 | 1.85% | 205–250 | melee weapons 48.0%, armor 31.8% |
| 6 | 15,041 | 4.69% | 255–300 | melee weapons 41.5%, missile weapons 30.3% |
| 7 | 34,539 | 10.77% | 301–355 | melee weapons 65.0%, missile weapons 14.6% |
| 8 | 34,124 | 10.65% | 360–1001 | melee weapons 65.3%, missile weapons 21.5% |

The ZIP's tier labels therefore cannot be equated to the generator tier in
`TreasureDeathRowV1`. In particular, tier 1 has 54% of the observations while
tier 2 has 0.22%; its median values are 8,532 and 293 respectively. These
ratios reflect the captured container/sample mix and classification method as
well as any loot rules.

## Comparison with current ACE source data

Pinned ACE `47edade3bd3f6044b676d4eb877c4965c7eda62b` selects the item,
magic-item and mundane categories using each source `TreasureDeath` profile,
then follows ordered nested chance/reference tables to WCID leaves. It applies
the separate material, spell, workmanship and mutation pipelines afterward.
The C# table oracle checks 10,703 rows and exact `f32` bits. The imported world
already stores `TreasureDeath` and material rows in `.bace` namespaces 39–43.

Of the ZIP's 200 most frequent WCIDs, **195** appear in the direct WCID leaves
of the pinned ACE table closure. They account for **207,845 of 210,390**
observations represented by the top-200 list (98.79%). The other five WCIDs are
45113, 44976, 45421, 45416 and 44977. This is a **direct-leaf** check only: fixed container contents,
creature create lists, scroll resolution, and other generation paths could
produce an item without listing it in those leaves.

## Gaps before balancing

- No source-container identity, kill/chest event denominator or profile mix is
  present, so an observed category share cannot identify an ACE drop chance.
- The ZIP does not define its tier classifier. Its wield bands suggest an item
  classification, while ACE treasure tier is an input to generation.
- Material/spell/stat summaries are marginal distributions; they cannot recover
  the conditional dependencies and RNG order of ACE's nested tables and mutation
  scripts.
- The five frequent WCIDs absent from direct leaves need classification by other
  source paths before treating them as table omissions.

The migration of literal ACE tables and mutation scripts to a versioned `.bace`
table set preserves the pinned baseline. A separate retail-calibrated profile
remains unconfigured until item-level evidence or a documented tier/source
mapping supports it.
