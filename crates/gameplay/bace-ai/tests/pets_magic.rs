use bace_ai::{CombatPetLifetime, MonsterSpell, MonsterSpellcasting, PetTarget, combat_pet_target};
#[test]
fn independently_compiled_gdle_spellbook_selection() {
    for row in include_str!("fixtures/spellcasting.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = row.split(',').collect();
        let mut ai = MonsterSpellcasting::new(
            vec![
                MonsterSpell {
                    spell: 100,
                    likelihood: 0.2,
                },
                MonsterSpell {
                    spell: 200,
                    likelihood: 0.8,
                },
            ],
            2.0,
        )
        .unwrap();
        let result = ai
            .select(1.0, &[f[0].parse().unwrap(), f[1].parse().unwrap()])
            .unwrap();
        assert_eq!(result.unwrap_or(0), f[2].parse::<u32>().unwrap(), "{row}");
        if result.is_some() {
            assert_eq!(ai.select(2.0, &[0.0, 0.0]).unwrap(), None);
            assert_eq!(ai.select(3.0, &[0.0, 0.0]).unwrap(), Some(100));
        }
    }
}
#[test]
fn pet_selects_nearest_eligible_monster_and_lifetime_is_explicit() {
    let base = PetTarget {
        id: 10,
        creature: true,
        player: false,
        combat_pet: false,
        alive: true,
        attackable: true,
        visible: true,
        same_faction: false,
        retaliating: false,
        distance_squared: 20.0,
    };
    let candidates = [
        base,
        PetTarget {
            id: 11,
            distance_squared: 1.0,
            ..base
        },
        PetTarget {
            id: 12,
            player: true,
            distance_squared: 0.0,
            ..base
        },
        PetTarget {
            id: 13,
            combat_pet: true,
            distance_squared: 0.0,
            ..base
        },
    ];
    assert_eq!(combat_pet_target(&candidates, 10.0).unwrap(), Some(11));
    let pet = CombatPetLifetime::new(1, 2, 45.0, 10.0).unwrap();
    assert!(!pet.expired(54.999, true));
    assert!(pet.expired(55.0, true));
    assert!(pet.expired(11.0, false));
}
