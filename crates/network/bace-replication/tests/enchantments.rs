use bace_gameplay_api::EnchantmentProjection;
use bace_replication::project_enchantments;

fn row(spell_id: u16, layer: u16, stat_value: f32) -> EnchantmentProjection {
    EnchantmentProjection {
        spell_id,
        layer,
        category: 23,
        power: 99,
        start_time: -15.0,
        duration: 60.0,
        caster_id: 0x50000001,
        degrade_modifier: 0.1,
        degrade_limit: 0.2,
        last_time_degraded: 0.0,
        stat_type: 0x02000000 | if spell_id == 100 { 0x4000 } else { 0x8000 },
        stat_key: 7,
        stat_value,
        spell_set_id: 42,
    }
}
fn fixture(name: &str) -> Vec<u8> {
    let hex = include_str!("fixtures/enchantments.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .find_map(|line| {
            line.split_once(',')
                .filter(|(key, _)| *key == name)
                .map(|(_, hex)| hex)
        })
        .unwrap();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}
#[test]
fn actual_pinned_ace_registry_writers_match_mixed_categories_and_vitae_layer() {
    let additive = row(101, 7, 12.5);
    let multiplicative = row(100, 3, 0.75);
    let mut cooldown = row(0x8001, 2, 35.0);
    cooldown.category = 0x8000;
    cooldown.power = 0;
    cooldown.degrade_modifier = 0.3;
    cooldown.degrade_limit = 0.4;
    cooldown.last_time_degraded = -2.0;
    cooldown.stat_type = 0x1000000;
    cooldown.stat_key = 22;
    cooldown.spell_set_id = 0;
    let mut vitae = row(666, 1, 0.95);
    vitae.duration = -1.0;
    for (name, rows) in [
        ("empty", vec![]),
        ("mixed", vec![additive, cooldown, vitae, multiplicative]),
        ("cooldown", vec![cooldown]),
        ("vitae", vec![vitae]),
    ] {
        let projected = project_enchantments(&rows, 16).unwrap();
        assert_eq!(projected.encode(16, 4096).unwrap(), fixture(name), "{name}");
    }
}
#[test]
fn projection_rejects_corruption_and_requires_domain_vitae_cleanup() {
    let mut value = row(100, 1, 0.75);
    assert!(project_enchantments(&[value, value], 16).is_err());
    assert!(project_enchantments(&[value], 0).is_err());
    value.duration = -2.0;
    assert!(project_enchantments(&[value], 16).is_err());
    let mut vitae = row(666, 1, 1.0);
    assert!(project_enchantments(&[vitae], 16).is_err());
    vitae.stat_value = 0.99995;
    assert!(project_enchantments(&[vitae], 16).is_err());
}
