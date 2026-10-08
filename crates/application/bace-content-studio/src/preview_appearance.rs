//! Explicit, undoable transfer of preview controls into native weenie properties.
use crate::document::Document;

#[derive(Clone, Copy)]
pub(crate) struct AppearanceSelection {
    pub setup: u32,
    pub clothing: u32,
    pub palette: u32,
    pub template: u32,
    pub shade: f64,
}

impl AppearanceSelection {
    pub fn validate(self) -> Result<Self, String> {
        if self.setup >> 24 != 2
            || self.setup & 0xffffff == 0
            || (self.clothing != 0 && (self.clothing >> 24 != 16 || self.clothing & 0xffffff == 0))
            || (self.palette != 0 && (self.palette >> 24 != 4 || self.palette & 0xffffff == 0))
            || self.template > i32::MAX as u32
            || !self.shade.is_finite()
            || !(0.0..=1.0).contains(&self.shade)
        {
            return Err("Choose a Setup (02), ClothingBase (10 or 0), Palette (04 or 0), a template fitting i32, and a shade between 0 and 1.".into());
        }
        Ok(self)
    }

    pub fn apply(self, document: &mut Document) -> Result<(), String> {
        self.validate()?;
        let mut candidate = document.value.clone();
        let properties = candidate
            .get_mut("properties")
            .and_then(toml::Value::as_table_mut)
            .ok_or("Native document has no property table")?;
        for (id, value) in [(1, self.setup), (7, self.clothing), (6, self.palette)] {
            set(
                properties,
                "data_ids",
                id,
                toml::Value::Integer(value.into()),
            )?;
        }
        // ACE PropertyInt.PaletteTemplate = 3; PropertyFloat.Shade = 12.
        set(
            properties,
            "ints",
            3,
            toml::Value::Integer(self.template.into()),
        )?;
        set(properties, "floats", 12, toml::Value::Float(self.shade))?;
        let text = toml::to_string_pretty(&candidate).map_err(|e| e.to_string())?;
        if text.len() > 16 * 1024 * 1024 {
            return Err("Editor document exceeds 16 MiB.".into());
        }
        bace_content_tools::parse(&text).map_err(|e| e.to_string())?;
        document.value = candidate;
        document.checkpoint()
    }
}

fn set(
    properties: &mut toml::Table,
    family: &str,
    id: i64,
    value: toml::Value,
) -> Result<(), String> {
    let rows = properties
        .entry(family)
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or("Native property family is not an array")?;
    if let Some(row) = rows
        .iter_mut()
        .find(|p| p.get("id").and_then(toml::Value::as_integer) == Some(id))
    {
        let row = row
            .as_table_mut()
            .ok_or("Native property row is not a table")?;
        row.insert("value".into(), value);
    } else {
        let mut row = toml::Table::new();
        row.insert("id".into(), toml::Value::Integer(id));
        row.insert("value".into(), value);
        rows.push(toml::Value::Table(row));
    }
    Ok(())
}
