//! Admission-only fixture dependencies. Numeric effect rows come from pinned
//! GDLE353cbab Bin/Data/json/spells.json IDs5938..5945; motion/formula execution
//! is intentionally outside these ownership tests. Real DAT closure has separate
//! runtime asset tests. Source data preserves its upstream attribution/license.
pub fn batch() -> bace_simulation::PreparedMagicAssetBatch {
    use bace_magic::*;
    use bace_simulation::{PreparedMagicAssetBatch, PreparedMagicDefinition, PreparedMagicSpell};
    let rows = [
        (5938, 684, 4, 1, 130, 98320, 45, -20.),
        (5939, 685, 2, 1, 65666, 36868, 318, 20.),
        (5940, 686, 4, 1, 130, 163856, 15, -20.),
        (5941, 687, 4, 1, 130, 36868, 317, 30.),
        (5942, 684, 4, 1, 130, 98320, 45, -20.),
        (5943, 685, 2, 250, 65666, 36868, 318, 12.),
        (5944, 686, 4, 1, 130, 163856, 15, -10.),
        (5945, 687, 4, 1, 130, 36868, 317, 20.),
    ];
    PreparedMagicAssetBatch {
        projectile_shapes: Vec::new(),
        definitions: rows
            .into_iter()
            .map(
                |(id, category, school, power, flags, stat_type, stat_key, value)| {
                    PreparedMagicDefinition {
                        spell: PreparedMagicSpell {
                            spell: PreparedSpell {
                                id,
                                school: if school == 2 {
                                    MagicSchool::Life
                                } else {
                                    MagicSchool::Creature
                                },
                                power,
                                base_mana: 0,
                                range_constant: 5.,
                                range_per_skill: 0.25,
                                harmful: true,
                                resistable: true,
                                effect: SpellEffect::Enchantment(EnchantmentSpec {
                                    category,
                                    power,
                                    duration: 20.,
                                    layer: 0,
                                    stat_type,
                                    stat_key,
                                    value,
                                    beneficial: false,
                                    set_id: None,
                                }),
                            },
                            gestures: Vec::new(),
                            components: Vec::new(),
                            component_modifiers: Vec::new(),
                            component_loss: 0.,
                            fast_resistable_pk_spell: false,
                        },
                        metadata: Some(EnchantmentMetadata {
                            enchantment_category: 1,
                            degrade_limit: -666.,
                            last_time_degraded: -1.,
                            ..Default::default()
                        }),
                        formula_level: 1,
                        category: u32::from(category),
                        flags,
                        target_mask: 16,
                    }
                },
            )
            .collect(),
    }
}
