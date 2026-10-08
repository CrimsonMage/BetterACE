use super::*;
#[test]
fn activation_mana_and_spell_gate_match_unchanged_ace_method() {
    let mut rows = 0;
    for line in include_str!("../../tests/fixtures/equipment_activation.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = line.split_whitespace().collect();
        let b = |i: usize| f[i] == "True";
        let mana = f[0].parse().unwrap();
        let (after, affecting, cast) = activation_properties(Some(mana), Some(b(1)), b(2));
        assert_eq!(after, Some(f[5].parse().unwrap()), "{line}");
        assert_eq!(affecting, Some(b(6)), "{line}");
        let expected_casts = usize::from(cast && !b(4));
        assert_eq!(expected_casts, f[7].parse::<usize>().unwrap(), "{line}");
        rows += 1;
    }
    assert_eq!(rows, 48);
    // The source caller only invokes activation for positive mana. Preserve
    // absent/zero/negative values and absent affecting state without invention.
    for mana in [None, Some(0), Some(-1)] {
        assert_eq!(activation_properties(mana, None, true), (mana, None, false));
    }
}
