use super::*;
fn item(id: u32, style: bool) -> PreparedItemExperience {
    let actor = EntityId(100);
    let tiers = (0..=6)
        .map(|tier| {
            let spells = if tier == 0 {
                vec![]
            } else if tier == 1 {
                vec![100]
            } else {
                vec![tier * 100, 900]
            };
            (
                tier,
                spells
                    .into_iter()
                    .map(|spell| PreparedGeneratorEnchantment {
                        target: actor,
                        entry: bace_magic::EnchantmentEntry {
                            spell,
                            caster: id,
                            school: bace_magic::MagicSchool::Creature,
                            spec: bace_magic::EnchantmentSpec {
                                category: spell as u16,
                                power: 100,
                                duration: 60.,
                                layer: 1,
                                stat_type: 4 | 0x8000,
                                stat_key: 1,
                                value: 1.,
                                beneficial: true,
                                set_id: Some(1),
                            },
                            start_time: 0.,
                            is_set_spell: true,
                            is_level8_aura: false,
                            metadata: bace_magic::EnchantmentMetadata {
                                has_spell_set_id: true,
                                spell_set_id: 1,
                                ..Default::default()
                            },
                        },
                    })
                    .collect(),
            )
        })
        .collect();
    PreparedItemExperience {
        item: EntityId(id),
        actor,
        name: format!("item{id}"),
        experience: Some(bace_character::ItemExperience {
            total: u64::from(id),
            base: 1,
            maximum_level: 6,
            style: bace_character::ItemExperienceStyle::Fixed,
            revision: 1,
        }),
        set: Some(crate::PreparedItemSet { id: 1, tiers }),
        set_uses_item_levels: style,
        equipment_order: u64::from(id),
    }
}
#[test]
fn equipment_set_deltas_and_surrogate_casters_match_original_ace_methods() {
    let mut count = 0;
    for line in include_str!("../../../../tests/fixtures/equipment_sets.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut f = line.splitn(5, ' ');
        let equip = f.next().unwrap() == "equip";
        let style = f.next().unwrap() == "1";
        let size: u32 = f.next().unwrap().parse().unwrap();
        let changed: u32 = f.next().unwrap().parse().unwrap();
        let expected = f.next().unwrap_or("");
        let all: Vec<_> = (1..=size).map(|id| item(id, style)).collect();
        let without: Vec<_> = all
            .iter()
            .filter(|p| p.item != EntityId(changed))
            .cloned()
            .collect();
        let (before, after) = if equip {
            (&without, &all)
        } else {
            (&all, &without)
        };
        let mut registry = EnchantmentRegistry::new(64).unwrap();
        for entry in tier(&before.iter().collect::<Vec<_>>()).unwrap() {
            registry.add(entry.entry, 0., true).unwrap();
        }
        let mut magic = crate::magic::Magic::new(64);
        magic
            .register_registry(EntityId(100), registry, true, 0.)
            .unwrap();
        let original = magic.registry(EntityId(100)).unwrap().entries().to_vec();
        let mut patches = Vec::new();
        update_set(
            &magic,
            &mut patches,
            EntityId(100),
            &item(changed, style),
            before,
            after,
            equip,
            1.,
        )
        .unwrap();
        let mut actual = Vec::new();
        for patch in &patches {
            for event in &patch.events {
                match event {
                    crate::MagicEvent::EnchantmentsRemoved { entries, .. } => {
                        actual.extend(entries.iter().map(|(id, _)| format!("-{id}")))
                    }
                    crate::MagicEvent::Enchantment { entry, .. } => {
                        actual.push(format!("+{}@{}", entry.spell, entry.caster))
                    }
                    _ => panic!("unexpected equipment registry effect"),
                }
            }
        }
        assert_eq!(actual.join(","), expected, "{line}");
        assert_eq!(
            magic.registry(EntityId(100)).unwrap().entries(),
            original,
            "proposal must not mutate owner"
        );
        count += 1;
    }
    assert_eq!(count, 28);
}
