use bace_magic::*;
fn entry(spell: u32, has_set: bool) -> EnchantmentEntry {
    EnchantmentEntry {
        spell,
        caster: if spell < 20 { 101 } else { 999 },
        school: MagicSchool::Item,
        spec: EnchantmentSpec {
            category: spell as u16,
            power: 100,
            duration: if spell == 20 { 60.0 } else { -1.0 },
            layer: 1,
            stat_type: 0,
            stat_key: 0,
            value: 1.0,
            beneficial: true,
            set_id: has_set.then_some(1),
        },
        start_time: -2.5,
        is_set_spell: has_set,
        is_level8_aura: false,
        metadata: EnchantmentMetadata {
            has_spell_set_id: has_set,
            spell_set_id: 1,
            ..Default::default()
        },
    }
}
#[test]
fn item_spell_login_audit_matches_unchanged_pinned_ace_method() {
    for line in include_str!("fixtures/audit.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let (case, expected) = line.split_once(',').unwrap();
        let case: u32 = case.parse().unwrap();
        let mut registry = EnchantmentRegistry::restore(
            16,
            8,
            vec![
                entry(10, case & 1 != 0),
                entry(11, case & 1 != 0),
                entry(20, false),
                entry(666, false),
            ],
        )
        .unwrap();
        let owners = if case & 4 != 0 {
            vec![EquippedSpellOwner {
                id: 101,
                equipped: case & 2 != 0,
                set: Some(EquippedSpellSet {
                    id: 1,
                    possible: &[10, 11],
                    active: if case & 8 != 0 { &[10] } else { &[] },
                }),
            }]
        } else {
            vec![]
        };
        assert!(audit_equipped_spells(&mut registry, &owners).unwrap());
        assert_eq!(registry.revision(), 9);
        let actual = registry
            .entries()
            .iter()
            .map(|e| e.spell.to_string())
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(actual, expected, "scenario {case}");
        assert!(registry.entries().iter().all(|e| e.start_time == -2.5));
        assert!(!audit_equipped_spells(&mut registry, &owners).unwrap());
        assert_eq!(registry.revision(), 9);
    }
}
#[test]
fn invalid_equipment_or_revision_overflow_cannot_partially_clean_registry() {
    let mut registry = EnchantmentRegistry::restore(16, u64::MAX, vec![entry(10, false)]).unwrap();
    let before = registry.entries().to_vec();
    assert_eq!(
        audit_equipped_spells(&mut registry, &[]),
        Err(RegistryError::RevisionExhausted)
    );
    assert_eq!(registry.entries(), before);
    let bad = EquippedSpellOwner {
        id: 101,
        equipped: true,
        set: Some(EquippedSpellSet {
            id: 1,
            possible: &[10],
            active: &[11],
        }),
    };
    assert_eq!(
        audit_equipped_spells(&mut registry, &[bad]),
        Err(RegistryError::InvalidEntry)
    );
    assert_eq!(registry.entries(), before);
}
