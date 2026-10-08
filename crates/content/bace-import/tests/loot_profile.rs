use bace_content::DeathTreasureV1;
use bace_import::{export_loot_json, export_loot_sql, import_loot_json};

// Field names are from pinned ACE.Database/Models/World/TreasureDeath.cs.
#[test]
fn loot_files_use_legacy_field_names_and_validate_before_export() {
    let profile = DeathTreasureV1 {
        treasure_type: 4294967295,
        tier: 8,
        item_chance: 95,
        item_min_amount: 2,
        item_max_amount: 7,
        loot_quality_mod: 0.25,
        ..Default::default()
    };
    let json = export_loot_json(&profile).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["TreasureType"], 4294967295_u32);
    assert_eq!(value["ItemMinAmount"], 2);
    assert!(value.get("schema_version").is_none());
    assert_eq!(import_loot_json(&json).unwrap(), profile);
    let sql = export_loot_sql(&profile).unwrap();
    assert!(
        sql.contains("INSERT INTO `treasure_death` (`treasure_type`, `tier`, `loot_quality_mod`")
    );
    assert!(sql.contains("VALUES (4294967295, 8, 0.25, 0, 95, 2, 7,"));
    for bad in [
        DeathTreasureV1 {
            schema_version: 2,
            ..profile.clone()
        },
        DeathTreasureV1 {
            item_chance: 101,
            ..profile.clone()
        },
        DeathTreasureV1 {
            item_min_amount: 8,
            ..profile.clone()
        },
        DeathTreasureV1 {
            loot_quality_mod: f32::NAN,
            ..profile.clone()
        },
    ] {
        assert!(export_loot_json(&bad).is_err());
        assert!(export_loot_sql(&bad).is_err());
    }
}

#[test]
fn loot_import_rejects_duplicates_unknown_fields_and_partial_profiles() {
    let json = export_loot_json(&DeathTreasureV1::default()).unwrap();
    for extra in ["\"Tier\": 2", "\"tier\": 2", "\"NotAField\": 7"] {
        let text = format!("{{{extra},{}", &json[1..]);
        assert!(import_loot_json(&text).is_err());
    }
    assert!(import_loot_json("{\"TreasureType\": 1}").is_err());
    assert!(import_loot_json(&" ".repeat(65_537)).is_err());
}
