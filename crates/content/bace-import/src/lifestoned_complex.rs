use crate::{ImportError, lifestoned_helpers::*};
use serde_json::{Value, json};

pub(crate) fn emotes(value: Value) -> Result<Value, ImportError> {
    let mut result: Vec<Value> = Vec::new();
    for group in array(value, "emoteTable")? {
        let (key, values) = key_value(group, "emote category")?;
        for value in array(values, "emote category value")? {
            let mut value = object(value, "emote")?;
            let mut emote = rename(
                &mut value,
                &[
                    ("category", "category"),
                    ("probability", "probability"),
                    ("classID", "weenie_class_id"),
                    ("style", "style"),
                    ("substyle", "substyle"),
                    ("quest", "quest"),
                    ("vendorType", "vendor_type"),
                    ("minhealth", "min_health"),
                    ("maxhealth", "max_health"),
                ],
            );
            emote.insert("legacy_category_key".into(), key.clone());
            let mut actions = Vec::new();
            for action in array(value.remove("emotes").unwrap_or(Value::Null), "emotes")? {
                actions.push(emote_action(action)?);
            }
            emote.insert("actions".into(), actions.into());
            done(&value, "emote")?;
            result.push(emote.into());
        }
    }
    Ok(result.into())
}

fn emote_action(value: Value) -> Result<Value, ImportError> {
    let mut value = object(value, "emote action")?;
    let mut out = rename(
        &mut value,
        &[
            ("type", "type"),
            ("delay", "delay"),
            ("extent", "extent"),
            ("amount", "amount"),
            ("motion", "motion"),
            ("msg", "message"),
            ("amount64", "amount64"),
            ("heroxp64", "hero_xp64"),
            ("min64", "min64"),
            ("max64", "max64"),
            ("percent", "percent"),
            ("max", "max"),
            ("min", "min"),
            ("fmax", "max_dbl"),
            ("fmin", "min_dbl"),
            ("stat", "stat"),
            ("pscript", "p_script"),
            ("sound", "sound"),
            ("spellid", "spell_id"),
            ("teststring", "test_string"),
            ("wealth_rating", "wealth_rating"),
            ("treasure_class", "treasure_class"),
            ("treasure_type", "treasure_type"),
        ],
    );
    if let Some(value) = optional(&mut value, "display") {
        out.insert("display".into(), flag(value)?);
    }
    if let Some(value) = optional(&mut value, "cprof") {
        out.extend(creation(value, true)?);
    }
    if let Some(value) = optional(&mut value, "frame") {
        out.extend(frame(value, false)?);
    }
    if let Some(position) = optional(&mut value, "mPosition") {
        let mut position = object(position, "mPosition")?;
        let translated = frame(take(&mut position, "frame")?, false)?;
        for (key, new_value) in translated {
            if out.get(&key).is_some_and(|old| old != &new_value) {
                return Err(ImportError::Unsupported(
                    "conflicting frame and mPosition".into(),
                ));
            }
            out.insert(key, new_value);
        }
        out.insert("obj_cell_id".into(), take(&mut position, "objcell_id")?);
        done(&position, "mPosition")?;
    }
    done(&value, "emote action")?;
    Ok(out.into())
}

pub(crate) fn generators(value: Value) -> Result<Value, ImportError> {
    let mut result: Vec<Value> = Vec::new();
    for value in array(value, "generatorTable")? {
        let mut value = object(value, "generator")?;
        let mut out = rename(
            &mut value,
            &[
                ("delay", "delay"),
                ("initCreate", "init_create"),
                ("maxNum", "max_create"),
                ("objcell_id", "obj_cell_id"),
                ("probability", "probability"),
                ("ptid", "palette_id"),
                ("shade", "shade"),
                ("slot", "legacy_slot"),
                ("stackSize", "stack_size"),
                ("type", "weenie_class_id"),
                ("whenCreate", "when_create"),
                ("whereCreate", "where_create"),
            ],
        );
        if let Some(value) = optional(&mut value, "frame") {
            out.extend(frame(value, false)?);
        }
        done(&value, "generator")?;
        result.push(out.into());
    }
    Ok(result.into())
}

pub(crate) fn body(value: Value) -> Result<Value, ImportError> {
    let mut value = object(value, "body")?;
    let parts = value.remove("body_part_table").unwrap_or(Value::Null);
    done(&value, "body")?;
    let mut result: Vec<Value> = Vec::new();
    for part in array(parts, "body parts")? {
        let (id, value) = key_value(part, "body part")?;
        let mut value = object(value, "body part value")?;
        let mut out = rename(
            &mut value,
            &[
                ("dtype", "d_type"),
                ("dval", "d_val"),
                ("dvar", "d_var"),
                ("bh", "bh"),
            ],
        );
        if let Some(armor) = optional(&mut value, "acache") {
            let mut armor = object(armor, "armor cache")?;
            out.extend(rename(
                &mut armor,
                &[
                    ("base_armor", "base_armor"),
                    ("armor_vs_slash", "armor_vs_slash"),
                    ("armor_vs_pierce", "armor_vs_pierce"),
                    ("armor_vs_bludgeon", "armor_vs_bludgeon"),
                    ("armor_vs_cold", "armor_vs_cold"),
                    ("armor_vs_fire", "armor_vs_fire"),
                    ("armor_vs_acid", "armor_vs_acid"),
                    ("armor_vs_electric", "armor_vs_electric"),
                    ("armor_vs_nether", "armor_vs_nether"),
                ],
            ));
            done(&armor, "armor cache")?;
        }
        if let Some(zones) = optional(&mut value, "bpsd") {
            let mut zones = object(zones, "body zones")?;
            out.extend(rename(
                &mut zones,
                &[
                    ("HLF", "hlf"),
                    ("MLF", "mlf"),
                    ("LLF", "llf"),
                    ("HRF", "hrf"),
                    ("MRF", "mrf"),
                    ("LRF", "lrf"),
                    ("HLB", "hlb"),
                    ("MLB", "mlb"),
                    ("LLB", "llb"),
                    ("HRB", "hrb"),
                    ("MRB", "mrb"),
                    ("LRB", "lrb"),
                ],
            ));
            done(&zones, "body zones")?;
        }
        done(&value, "body part value")?;
        result.push(json!({"id":id,"value":out}));
    }
    Ok(result.into())
}
