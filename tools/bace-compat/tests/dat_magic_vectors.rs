use bace_dat::{DatTableLimits, SpellComponents, SpellTable, spell_hash_cp1252};
use serde_json::Value;
fn fixture() -> Value {
    let f: Value = serde_json::from_str(include_str!("../fixtures/dat_magic.json")).unwrap();
    assert_eq!(f["commit"], bace_compat::ACE_COMMIT);
    f["vectors"].clone()
}
fn bytes(v: &Value) -> Vec<u8> {
    v.as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn official_dat_spell_fields_obfuscated_cp1252_and_formula_versions() {
    let f = fixture();
    let input = bytes(&f["table"]["bytes"]);
    let table = SpellTable::decode(&input).unwrap();
    for (id, spell) in &table.spells {
        let e = &f["table"]["spells"][id.to_string()];
        assert_eq!(spell.name, e["Name"]);
        assert_eq!(spell.description, e["Desc"]);
        assert_eq!(
            spell.school,
            u32::try_from(e["School"].as_u64().unwrap()).unwrap()
        );
        assert_eq!(spell.meta_type, e["MetaSpellType"].as_u64().unwrap() as u32);
        assert_eq!(
            spell.formula,
            e["Formula"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect::<Vec<_>>()
        );
        if let Some((duration, degrade, limit)) = spell.enchantment {
            assert_eq!(duration, e["Duration"].as_f64().unwrap());
            assert_eq!(degrade, e["DegradeModifier"].as_f64().unwrap() as f32);
            assert_eq!(limit, e["DegradeLimit"].as_f64().unwrap() as f32);
        }
    }
    for row in f["formulas"].as_array().unwrap() {
        let account = row["account"].as_str().unwrap();
        let cp: Vec<u8> = account.chars().map(|c| c as u8).collect();
        let actual = table.spells[&(row["spell"].as_u64().unwrap() as u32)]
            .formula_for_account(&cp)
            .unwrap();
        assert_eq!(
            actual,
            row["formula"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect::<Vec<_>>(),
            "{row}"
        );
    }
    assert_eq!(table.set_at_level(7, 0), None);
    assert_eq!(table.set_at_level(7, 2), Some(&[100, 101][..]));
    assert_eq!(table.set_at_level(7, u32::MAX), Some(&[102][..]));
    for end in 0..input.len() {
        assert!(SpellTable::decode(&input[..end]).is_err());
    }
    assert!(
        SpellTable::decode_with_limits(
            &input,
            DatTableLimits {
                max_entries: 1,
                ..Default::default()
            }
        )
        .is_err()
    );
    let hashes = [b"Alpha".as_slice(), b"Caf\xe9", b"\x96 \x80", b""];
    for (raw, row) in hashes.iter().zip(f["hashes"].as_array().unwrap()) {
        assert_eq!(
            spell_hash_cp1252(raw),
            row["value"].as_u64().unwrap() as u32
        );
    }
}
#[test]
fn official_spell_components_preserve_gesture_time_and_words() {
    let f = fixture();
    let input = bytes(&f["components"]["bytes"]);
    let table = SpellComponents::decode(&input).unwrap();
    for (id, c) in &table.components {
        let e = &f["components"]["entries"][id.to_string()];
        assert_eq!(c.name, e["Name"]);
        assert_eq!(c.text, e["Text"]);
        assert_eq!(c.gesture, e["Gesture"].as_u64().unwrap() as u32);
        assert_eq!(c.time, e["Time"].as_f64().unwrap() as f32);
        assert_eq!(c.modifier, e["CDM"].as_f64().unwrap() as f32);
    }
    for end in 0..input.len() {
        assert!(SpellComponents::decode(&input[..end]).is_err());
    }
}
