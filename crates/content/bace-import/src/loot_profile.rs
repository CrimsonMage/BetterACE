//! Legacy loot-profile files; no SQL connections or live publication.
use bace_content::DeathTreasureV1;
const FIELDS: [(&str, &str); 16] = [
    ("TreasureType", "treasure_type"),
    ("Tier", "tier"),
    ("LootQualityMod", "loot_quality_mod"),
    ("UnknownChances", "unknown_chances"),
    ("ItemChance", "item_chance"),
    ("ItemMinAmount", "item_min_amount"),
    ("ItemMaxAmount", "item_max_amount"),
    (
        "ItemTreasureTypeSelectionChances",
        "item_treasure_type_selection_chances",
    ),
    ("MagicItemChance", "magic_item_chance"),
    ("MagicItemMinAmount", "magic_item_min_amount"),
    ("MagicItemMaxAmount", "magic_item_max_amount"),
    (
        "MagicItemTreasureTypeSelectionChances",
        "magic_item_treasure_type_selection_chances",
    ),
    ("MundaneItemChance", "mundane_item_chance"),
    ("MundaneItemMinAmount", "mundane_item_min_amount"),
    ("MundaneItemMaxAmount", "mundane_item_max_amount"),
    (
        "MundaneItemTypeSelectionChances",
        "mundane_item_type_selection_chances",
    ),
];
pub fn import_loot_json(text: &str) -> Result<DeathTreasureV1, String> {
    if text.len() > 64 * 1024 {
        return Err("Loot profile exceeds 64 KiB".into());
    }
    let value = serde_json::from_str::<crate::strict_json::StrictJson>(text)
        .map_err(|e| e.to_string())?
        .0;
    let object = value
        .as_object()
        .ok_or("Expected one loot profile object")?;
    let mut native = serde_json::Map::new();
    native.insert("schema_version".into(), 1.into());
    for (name, value) in object {
        let normalized = name.replace('_', "").to_ascii_lowercase();
        let (_,key)=FIELDS.iter().find(|(legacy,_)|legacy.to_ascii_lowercase()==normalized).ok_or_else(||format!("Unsupported loot-profile field {name}; metadata must remain in native authoring files"))?;
        if native.insert((*key).into(), value.clone()).is_some() {
            return Err(format!("Duplicate loot-profile field {name}"));
        }
    }
    let profile: DeathTreasureV1 =
        serde_json::from_value(native.into()).map_err(|e| e.to_string())?;
    profile.validate()?;
    Ok(profile)
}
pub fn export_loot_json(profile: &DeathTreasureV1) -> Result<String, String> {
    profile.validate()?;
    let native = serde_json::to_value(profile).map_err(|e| e.to_string())?;
    let legacy: serde_json::Map<_, _> = FIELDS
        .iter()
        .map(|(old, key)| (old.to_string(), native[*key].clone()))
        .collect();
    serde_json::to_string_pretty(&legacy).map_err(|e| e.to_string())
}
pub fn export_loot_sql(profile: &DeathTreasureV1) -> Result<String, String> {
    profile.validate()?;
    let native = serde_json::to_value(profile).map_err(|e| e.to_string())?;
    let columns = FIELDS
        .iter()
        .map(|(_, key)| format!("`{key}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let values = FIELDS
        .iter()
        .map(|(_, key)| native[*key].to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "-- BetterACE legacy death-treasure export; destination assigns relational ID/timestamp.\nINSERT INTO `treasure_death` ({columns}) VALUES ({values});\n"
    ))
}
