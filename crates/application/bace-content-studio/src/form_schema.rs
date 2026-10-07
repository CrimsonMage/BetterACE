use bace_content::*;
use serde::Serialize;

pub(crate) struct Section {
    pub key: &'static str,
    pub label: &'static str,
    pub prototype: toml::Value,
    pub dictionary: bool,
}

fn value<T: Serialize>(item: T) -> Result<toml::Value, String> {
    toml::Value::try_from(item).map_err(|e| e.to_string())
}

pub(crate) fn sections() -> Result<Vec<Section>, String> {
    let mut sections = Vec::new();
    macro_rules! dict {
        ($key:literal,$label:literal,$value:expr) => {
            sections.push(Section {
                key: $key,
                label: $label,
                prototype: value(Property {
                    id: 1_u32,
                    value: $value,
                })?,
                dictionary: true,
            });
        };
    }
    macro_rules! seq {
        ($key:literal,$label:literal,$value:expr) => {
            sections.push(Section {
                key: $key,
                label: $label,
                prototype: value($value)?,
                dictionary: false,
            });
        };
    }
    dict!("ints", "Integer 32", 0_i32);
    dict!("int64s", "Integer 64", 0_i64);
    dict!("bools", "Boolean", false);
    dict!("floats", "Float", 0.0_f64);
    dict!("strings", "String", String::new());
    dict!("data_ids", "Data IDs", 0_u32);
    dict!("instance_ids", "Instance IDs", 0_u32);
    dict!("attributes", "Attributes", Attribute::default());
    dict!(
        "secondary_attributes",
        "Vitals",
        SecondaryAttribute::default()
    );
    dict!("skills", "Skills", Skill::default());
    dict!("body_parts", "Body parts", BodyPart::default());
    dict!("spell_book", "Spell book", 0.0_f32);
    seq!("create_list", "Create items", CreateListEntry::default());
    seq!("emotes", "Emotes", Emote::default());
    seq!("book_pages", "Book pages", BookPage::default());
    seq!("generators", "Generators", Generator::default());
    dict!(
        "positions",
        "Positions",
        Position {
            rotation_w: 1.0,
            ..Default::default()
        }
    );
    seq!(
        "animation_parts",
        "Animation parts",
        AnimationPart::default()
    );
    seq!("palettes", "Palettes", Palette::default());
    seq!("texture_maps", "Texture maps", TextureMap::default());
    seq!("event_filter", "Event filters", 0_i32);
    Ok(sections)
}

/// Populate absent optionals in a prototype only. Existing authored absence
/// stays absent until the user explicitly chooses to add a field.
pub(crate) fn optional(kind: &str) -> Result<toml::Value, String> {
    match kind {
        "emotes" => value(Emote {
            legacy_category_key: Some(0),
            weenie_class_id: Some(0),
            style: Some(0),
            substyle: Some(0),
            quest: Some(String::new()),
            vendor_type: Some(0),
            min_health: Some(0.0),
            max_health: Some(1.0),
            ..Default::default()
        }),
        "actions" => value(EmoteAction {
            legacy_order: Some(0),
            motion: Some(0),
            message: Some(String::new()),
            test_string: Some(String::new()),
            min: Some(0),
            max: Some(0),
            min64: Some(0),
            max64: Some(0),
            min_dbl: Some(0.0),
            max_dbl: Some(0.0),
            stat: Some(0),
            display: Some(false),
            amount: Some(0),
            amount64: Some(0),
            hero_xp64: Some(0),
            percent: Some(0.0),
            spell_id: Some(0),
            wealth_rating: Some(0),
            treasure_class: Some(0),
            treasure_type: Some(0),
            p_script: Some(0),
            sound: Some(0),
            destination_type: Some(0),
            weenie_class_id: Some(0),
            stack_size: Some(0),
            palette: Some(0),
            shade: Some(0.0),
            try_to_bond: Some(false),
            obj_cell_id: Some(0),
            origin_x: Some(0.0),
            origin_y: Some(0.0),
            origin_z: Some(0.0),
            angles_w: Some(1.0),
            angles_x: Some(0.0),
            angles_y: Some(0.0),
            angles_z: Some(0.0),
            ..Default::default()
        }),
        "generators" => value(Generator {
            legacy_slot: Some(0),
            delay: Some(0.0),
            stack_size: Some(0),
            palette_id: Some(0),
            shade: Some(0.0),
            obj_cell_id: Some(0),
            origin_x: Some(0.0),
            origin_y: Some(0.0),
            origin_z: Some(0.0),
            angles_w: Some(1.0),
            angles_x: Some(0.0),
            angles_y: Some(0.0),
            angles_z: Some(0.0),
            ..Default::default()
        }),
        "book_pages" => value(BookPage {
            legacy_page_id: Some(0),
            author_name: Some(String::new()),
            author_account: Some(String::new()),
            page_text: Some(String::new()),
            ..Default::default()
        }),
        _ => Ok(toml::Value::Table(toml::Table::new())),
    }
}

pub(crate) fn action() -> Result<toml::Value, String> {
    value(EmoteAction::default())
}
pub(crate) fn book() -> Result<toml::Value, String> {
    value(Book::default())
}
