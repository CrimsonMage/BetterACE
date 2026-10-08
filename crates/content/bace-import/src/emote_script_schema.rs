use serde_json::Value;

pub(crate) fn rows() -> impl Iterator<Item = [&'static str; 4]> {
    static ROWS: std::sync::OnceLock<Vec<[&'static str; 4]>> = std::sync::OnceLock::new();
    ROWS.get_or_init(|| {
        include_str!("../data/emotescript-schema.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|line| {
                let row: Vec<_> = line.split('\t').collect();
                row.try_into().ok()
            })
            .collect()
    })
    .iter()
    .copied()
}
pub(crate) fn metadata(kind: &str, family: &str, name: &str) -> Option<&'static str> {
    type Index = std::collections::BTreeMap<(String, String, String), &'static str>;
    static INDEX: std::sync::OnceLock<Index> = std::sync::OnceLock::new();
    INDEX
        .get_or_init(|| {
            rows()
                .map(|r| ((r[0].into(), r[1].into(), r[2].to_ascii_lowercase()), r[3]))
                .collect()
        })
        .get(&(kind.into(), family.into(), name.to_ascii_lowercase()))
        .copied()
}
pub(crate) fn enum_id(family: &str, name: &str) -> Option<u32> {
    let name = name.trim();
    name.parse()
        .ok()
        .or_else(|| {
            name.strip_prefix("0x")
                .and_then(|s| u32::from_str_radix(s, 16).ok())
        })
        .or_else(|| metadata("enum", family, name).and_then(|s| s.parse().ok()))
        .or_else(|| crate::enum_names::resolve(family, name))
        .or_else(|| crate::property_names::resolve(family, name))
}
pub(crate) fn enum_name(family: &str, id: i64) -> String {
    rows()
        .find(|r| r.len() == 4 && r[0] == "enum" && r[1] == family && r[3].parse::<i64>() == Ok(id))
        .map(|r| r[2].to_string())
        .unwrap_or_else(|| id.to_string())
}
pub(crate) fn key(field: &str) -> String {
    match field {
        "MinFloat" => "min_dbl".into(),
        "MaxFloat" => "max_dbl".into(),
        "Pscript" | "PScript" => "p_script".into(),
        "HeroXP64" => "hero_xp64".into(),
        _ => {
            let mut out = String::new();
            for (i, c) in field.chars().enumerate() {
                if c.is_uppercase() && i > 0 {
                    out.push('_');
                }
                out.extend(c.to_lowercase());
            }
            out
        }
    }
}
pub(crate) fn field(family: &str, name: &str) -> Option<(&'static str, &'static str)> {
    rows()
        .find(|r| {
            r.len() == 4
                && r[0] == "field"
                && r[1] == family
                && (r[2].eq_ignore_ascii_case(name) || key(r[2]) == name)
        })
        .map(|r| (r[2], r[3]))
}
pub(crate) fn parse_value(typ: &str, text: &str) -> Result<Value, String> {
    let text = text.trim();
    if typ == "String" {
        return if text.starts_with('"') {
            serde_json::from_str::<String>(text)
                .map(Value::String)
                .map_err(|e| e.to_string())
        } else {
            Ok(Value::String(text.into()))
        };
    }
    if typ == "Bool" {
        return match text.to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(Value::Bool(true)),
            "false" | "0" => Ok(Value::Bool(false)),
            _ => Err("Expected true or false".into()),
        };
    }
    if matches!(typ, "Float" | "Double") {
        let percent = text.ends_with('%');
        let n = text
            .trim_end_matches('%')
            .parse::<f64>()
            .map_err(|_| format!("Invalid number {text}"))?
            / if percent { 100.0 } else { 1.0 };
        return serde_json::Number::from_f64(n)
            .map(Value::Number)
            .ok_or_else(|| "Expected finite number".into());
    }
    let numeric = text
        .rsplit_once('(')
        .filter(|(_, n)| n.ends_with(')'))
        .map_or(text, |(_, n)| n.trim_end_matches(')'));
    let numeric = numeric.replace(',', "");
    let number = if matches!(typ, "Int32" | "Int64" | "UInt32") {
        numeric.parse::<i64>().ok().or_else(|| {
            text.strip_prefix("0x")
                .and_then(|s| i64::from_str_radix(s, 16).ok())
        })
    } else {
        enum_id(typ, text).map(i64::from)
    };
    number
        .map(Value::from)
        .ok_or_else(|| format!("Unknown {typ} value {text}; use its numeric ID"))
}
