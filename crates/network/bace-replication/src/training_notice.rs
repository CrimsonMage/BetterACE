//! Pinned ACE Skill.ToSentence and Player_Skills.HandleActionTrainSkill notices.
//! ACEmulator/ACE source attribution: tests/fixtures/training_notice.provenance.
use crate::ProgressionProjectionError;
pub(crate) const NAMES: [&str; 55] = [
    "None",
    "Axe",
    "Bow",
    "Crossbow",
    "Dagger",
    "Mace",
    "Melee Defense",
    "Missile Defense",
    "Sling",
    "Spear",
    "Staff",
    "Sword",
    "Thrown Weapon",
    "Unarmed Combat",
    "Arcane Lore",
    "Magic Defense",
    "Mana Conversion",
    "Spellcraft",
    "Item Tinkering",
    "Assess Person",
    "Deception",
    "Healing",
    "Jump",
    "Lockpick",
    "Run",
    "Awareness",
    "Arms And Armor Repair",
    "Assess Creature",
    "Weapon Tinkering",
    "Armor Tinkering",
    "Magic Item Tinkering",
    "Creature Enchantment",
    "Item Enchantment",
    "Life Magic",
    "War Magic",
    "Leadership",
    "Loyalty",
    "Fletching",
    "Alchemy",
    "Cooking",
    "Salvaging",
    "Two Handed Combat",
    "Gearcraft",
    "Void Magic",
    "Heavy Weapons",
    "Light Weapons",
    "Finesse Weapons",
    "Missile Weapons",
    "Shield",
    "Dual Wield",
    "Recklessness",
    "Sneak Attack",
    "Dirty Fighting",
    "Challenge",
    "Summoning",
];

/// Encode the source advancement notice after an accepted transition or a
/// source-defined training failure. This does not authorize or commit a skill.
pub fn project_training_notice(
    skill: u32,
    credits: u32,
    success: bool,
) -> Result<Vec<u8>, ProgressionProjectionError> {
    let name = NAMES
        .get(skill as usize)
        .ok_or(ProgressionProjectionError::InvalidProjection)?;
    if credits > i32::MAX as u32 {
        return Err(ProgressionProjectionError::InvalidProjection);
    }
    let text = if success {
        format!("{name} trained. You now have {credits} credits available.")
    } else {
        format!("Failed to train {name}! You now have {credits} credits available.")
    };
    bace_wire::ChatMessage::System {
        text: &text,
        chat_type: 13,
    }
    .encode()
    .map_err(|_| ProgressionProjectionError::InvalidProjection)
}
