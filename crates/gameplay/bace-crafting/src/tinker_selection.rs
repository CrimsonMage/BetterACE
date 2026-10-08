//! Source recipe fallback and foolproof template classification; no live I/O.
use bace_content::WeenieV1;
#[path = "tinker_selection_table.rs"]
mod table;
#[derive(Clone, Copy)]
pub(super) enum Rule {
    Value,
    Burden,
    Mana,
    Melee,
    Missile,
    Weapon,
    Caster,
    Workmanship,
    Jewelry,
    Armor,
    DefenseImbue,
    Any,
}
/// Called only after the exact source/target cookbook lookup misses. The caller
/// still validates selected recipe SalvageType and all imported requirements.
pub fn select_new_tinkering_recipe(source: &WeenieV1, target: &WeenieV1) -> Option<u32> {
    let (recipe, rule) = table::lookup(source.weenie_id)?;
    let int = |id| {
        target
            .properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value)
    };
    let work = int(105).is_some();
    let armor = int(28).unwrap_or(0) > 0;
    let material = source
        .properties
        .ints
        .iter()
        .find(|p| p.id == 131)
        .map(|p| p.value);
    let valid = match rule {
        Rule::Value => work && int(19).unwrap_or(0) != 0,
        Rule::Burden => work && int(5).unwrap_or(0) != 0,
        Rule::Mana => work && int(108).unwrap_or(0) != 0,
        Rule::Melee => work && target.weenie_type == 6,
        Rule::Missile => work && target.weenie_type == 3,
        Rule::Weapon => work && matches!(target.weenie_type, 3 | 6),
        Rule::Caster => work && target.weenie_type == 35,
        Rule::Workmanship => work,
        Rule::Jewelry => work && target.weenie_type == 1 && int(9) != Some(0x04000000),
        Rule::Armor => {
            work && armor
                && (int(1) == Some(2) || int(1) == Some(4) && matches!(int(9), Some(1 | 32 | 256)))
                && (material != Some(64) || int(36).unwrap_or(0) < 9999)
        }
        Rule::DefenseImbue => work && armor,
        Rule::Any => true,
    };
    valid.then_some(recipe)
}
pub fn is_foolproof_tinker(template: u32) -> bool {
    matches!(
        template,
        30094
            | 30095
            | 30096
            | 30097
            | 30098
            | 30099
            | 30100
            | 30101
            | 30102
            | 30103
            | 30104
            | 30105
            | 30106
            | 36619
            | 36620
            | 36621
            | 36622
            | 36623
            | 36624
            | 36625
            | 36626
            | 36627
            | 36628
            | 36634
            | 36635
            | 36636
    )
}
