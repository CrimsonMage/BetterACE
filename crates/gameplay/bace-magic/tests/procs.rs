use bace_magic::*;
#[test]
fn original_cloak_and_sigil_decision_vectors() {
    let policy = MagicProcPolicy::default();
    let mut counts = [0; 4];
    for row in include_str!("fixtures/procs.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut columns = row.split(',');
        let kind = columns.next().unwrap();
        let p: Vec<f64> = columns.map(|v| v.parse().unwrap()).collect();
        if kind == "school" {
            let school = [
                MagicSchool::War,
                MagicSchool::Life,
                MagicSchool::Creature,
                MagicSchool::Item,
                MagicSchool::Void,
            ][p[1] as usize];
            assert_eq!(
                MagicSchool::from_native_id(p[0] as u32),
                Some(school),
                "{row}"
            );
            assert_eq!(school.native_id(), p[0] as u32, "{row}");
            counts[3] += 1;
        } else if kind == "visual" {
            counts[2] += 1;
            assert_eq!(
                spell_projectile_intensity(p[0] as u32).to_bits(),
                (p[1] as f32).to_bits(),
                "{row}"
            );
        } else if kind == "cloak" {
            counts[0] += 1;
            let current_health = p[0] as u32;
            let p = &p[1..];
            let cloak = MagicCloak {
                item: 10,
                level: p[2] as u32,
                effect: if p[3] == 2. {
                    MagicCloakEffect::Absorb
                } else {
                    MagicCloakEffect::Spell(p[4] as u32)
                },
            };
            let result = magic_cloak_proc(MagicCloakInput {
                cloak,
                damage: p[0] as u32,
                current_health,
                player_vs_player: p[1] != 0.,
                owner: 1,
                current_enemy: (p[5] != 0.).then_some(99),
                roll: p[6],
                policy,
            })
            .unwrap();
            assert_eq!(result.damage, p[7] as u32, "{row}");
            assert_eq!(usize::from(result.cast.is_some()), p[8] as usize, "{row}");
            assert_eq!(result.cast.map_or(0, |c| c.target), p[9] as u32, "{row}");
        } else {
            assert_eq!(kind, "sigil");
            counts[1] += 1;
            let mut sigils = Vec::new();
            for slot in 0..3 {
                if p[0] as u32 & (1 << slot) != 0 {
                    sigils.push(MagicSigil {
                        item: 10 + slot,
                        slot: slot as u8,
                        level: slot + 1,
                        spell: if p[1] != 0. { 5208 } else { 100 + slot },
                    });
                }
            }
            let result = magic_sigil_procs(
                &sigils,
                1,
                (p[3] != 0.).then_some(p[3] as u32),
                &[p[4]; 3],
                policy,
                |_| Some(if p[2] != 0. { 8 } else { 0 }),
            )
            .unwrap();
            assert_eq!(result.len(), p[5] as usize, "{row}");
            for (actual, expected) in result.iter().zip(p[6..].chunks_exact(3)) {
                assert_eq!(
                    (actual.item, actual.target, actual.spell),
                    (expected[0] as u32, expected[1] as u32, expected[2] as u32),
                    "{row}"
                );
            }
        }
    }
    assert_eq!(counts, [1536, 72, 13, 5]);
}
