use bace_character::{inflict_vitae, vitae_experience};
#[test]
fn source_vitae_pool_registry_bits_and_delayed_removal_match() {
    for row in include_str!("fixtures/vitae.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let c: Vec<_> = row.split(',').collect();
        let value = f32::from_bits(c[1].parse().unwrap());
        let change = vitae_experience(
            value,
            c[2].parse().unwrap(),
            c[0].parse().unwrap(),
            c[3].parse().unwrap(),
        )
        .unwrap();
        assert_eq!(change.after_pool, c[4].parse::<i32>().unwrap(), "{row}");
        assert_eq!(
            change.after_value.to_bits(),
            c[5].parse::<u32>().unwrap(),
            "{row}"
        );
        assert_eq!(change.remove_after_seconds.is_some(), c[6] == "1", "{row}");
    }
}
#[test]
fn invalid_vitae_never_becomes_a_partial_registry_change() {
    assert!(vitae_experience(f32::NAN, 0, 100, 500).is_err());
    assert!(vitae_experience(0.95, -1, 100, 1).is_err());
    assert!(vitae_experience(0.95, 0, 0, 1).is_err());
    assert!(inflict_vitae(0, None, 0.05, 0.4).is_err());
    assert_eq!(inflict_vitae(1, None, 0.05, 0.4).unwrap(), 0.99);
}
