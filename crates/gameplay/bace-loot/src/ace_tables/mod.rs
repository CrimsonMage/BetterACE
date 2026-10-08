//! Pinned ACE literal rows loaded from the accepted versioned .bace table set.
//! Gameplay code owns roll semantics; this module owns immutable prepared data.
// Copyright ACE contributors. AGPL-3.0-only.
use bace_content::{TreasureTableSetV1, TreasureTableV1};
use std::{collections::BTreeMap, sync::OnceLock};

static ACTIVE: OnceLock<PreparedTables> = OnceLock::new();

struct PreparedTable {
    chance: Vec<(i64, f32)>,
    float_chance: Vec<(f32, f32)>,
    gem: Vec<(i64, i64, f32)>,
    sequence: Vec<i64>,
    references: Vec<String>,
    typed_references: Vec<(String, i64)>,
    float_sequence: Vec<f32>,
    descriptors: Vec<(i64, String)>,
}
struct PreparedTables {
    id: u32,
    tables: BTreeMap<String, BTreeMap<String, PreparedTable>>,
    enums: BTreeMap<String, BTreeMap<String, (i64, usize)>>,
    spell_routes: BTreeMap<i64, String>,
    template_leaf_tables: Vec<String>,
    scripts: Vec<(String, String)>,
}
impl From<TreasureTableV1> for PreparedTable {
    fn from(t: TreasureTableV1) -> Self {
        Self {
            chance: t
                .chance
                .into_iter()
                .map(|r| (r.value, f32::from_bits(r.probability_bits)))
                .collect(),
            float_chance: t
                .float_chance
                .into_iter()
                .map(|r| {
                    (
                        f32::from_bits(r.value_bits),
                        f32::from_bits(r.probability_bits),
                    )
                })
                .collect(),
            gem: t
                .gem
                .into_iter()
                .map(|r| (r.wcid, r.material, f32::from_bits(r.probability_bits)))
                .collect(),
            sequence: t.sequence,
            references: t.references,
            typed_references: t
                .typed_references
                .into_iter()
                .map(|r| (r.name, r.value))
                .collect(),
            float_sequence: t
                .float_sequence_bits
                .into_iter()
                .map(f32::from_bits)
                .collect(),
            descriptors: t
                .descriptors
                .into_iter()
                .map(|r| (r.name.parse().expect("validated descriptor"), r.text))
                .collect(),
        }
    }
}
/// Install once during cold startup, before any ACE treasure or enum lookup.
/// A new accepted table profile requires a game-child restart.
pub fn install(value: TreasureTableSetV1) -> Result<(), String> {
    value.validate()?;
    if ACTIVE.get().is_some() {
        return Err("ACE treasure tables already installed".into());
    }
    let mut tables = BTreeMap::<String, BTreeMap<String, PreparedTable>>::new();
    for table in value.tables {
        let class = table.class.clone();
        let field = table.field.clone();
        tables.entry(class).or_default().insert(field, table.into());
    }
    let mut enums = BTreeMap::<String, BTreeMap<String, (i64, usize)>>::new();
    for item in value.enums {
        let (kind, name) = item.name.split_once('.').ok_or("invalid ACE enum")?;
        let members = enums.entry(kind.to_owned()).or_default();
        let order = members.len();
        members.insert(name.to_owned(), (item.value, order));
    }
    let prepared = PreparedTables {
        id: value.id,
        tables,
        enums,
        spell_routes: value
            .spell_routes
            .into_iter()
            .map(|r| (r.value, r.name))
            .collect(),
        template_leaf_tables: value.template_leaf_tables,
        scripts: value
            .scripts
            .into_iter()
            .map(|r| (r.name, r.text))
            .collect(),
    };
    ACTIVE
        .set(prepared)
        .map_err(|_| "ACE treasure tables already installed".into())
}

pub fn active_id() -> Option<u32> {
    Some(ACTIVE.get()?.id)
}
fn table(class: &str, field: &str) -> Option<&'static PreparedTable> {
    ACTIVE.get()?.tables.get(class)?.get(field)
}
pub fn lookup(class: &str, field: &str) -> Option<&'static [(i64, f32)]> {
    let t = table(class, field)?;
    (!t.chance.is_empty()).then_some(t.chance.as_slice())
}
pub fn lookup_float(class: &str, field: &str) -> Option<&'static [(f32, f32)]> {
    let t = table(class, field)?;
    (!t.float_chance.is_empty()).then_some(t.float_chance.as_slice())
}
pub fn gem(class: &str, field: &str) -> Option<&'static [(i64, i64, f32)]> {
    let t = table(class, field)?;
    (!t.gem.is_empty()).then_some(t.gem.as_slice())
}
pub fn sequence(class: &str, field: &str) -> Option<&'static [i64]> {
    let t = table(class, field)?;
    (!t.sequence.is_empty()).then_some(t.sequence.as_slice())
}
pub fn references(class: &str, field: &str) -> Option<&'static [String]> {
    let t = table(class, field)?;
    (!t.references.is_empty()).then_some(t.references.as_slice())
}
pub fn typed_references(class: &str, field: &str) -> Option<&'static [(String, i64)]> {
    let t = table(class, field)?;
    (!t.typed_references.is_empty()).then_some(t.typed_references.as_slice())
}
pub fn float_sequence(class: &str, field: &str) -> Option<&'static [f32]> {
    let t = table(class, field)?;
    (!t.float_sequence.is_empty()).then_some(t.float_sequence.as_slice())
}
pub fn descriptors(class: &str, field: &str) -> Option<&'static [(i64, String)]> {
    let t = table(class, field)?;
    (!t.descriptors.is_empty()).then_some(t.descriptors.as_slice())
}
pub fn enum_value(kind: &str, name: &str) -> Option<i64> {
    Some(ACTIVE.get()?.enums.get(kind)?.get(name)?.0)
}
pub fn enum_members(kind: &str) -> Option<Vec<(&'static str, i64)>> {
    let mut members: Vec<_> = ACTIVE
        .get()?
        .enums
        .get(kind)?
        .iter()
        .map(|(name, &(value, order))| (order, name.as_str(), value))
        .collect();
    members.sort_unstable_by_key(|row| row.0);
    Some(
        members
            .into_iter()
            .map(|(_, name, value)| (name, value))
            .collect(),
    )
}
pub fn spell_progression(field: &str) -> Option<&'static [i64]> {
    sequence("SpellLevelProgression", field)
}
pub fn spell_levels(spell: i64) -> Option<&'static [i64]> {
    let name = ACTIVE.get()?.spell_routes.get(&spell)?;
    let (class, field) = name.split_once('.')?;
    sequence(class, field)
}
pub fn script_sources() -> Option<&'static [(String, String)]> {
    Some(&ACTIVE.get()?.scripts)
}
pub fn template_leaf_tables() -> Option<&'static [String]> {
    Some(&ACTIVE.get()?.template_leaf_tables)
}

#[cfg(test)]
pub(crate) fn install_test() {
    if active_id() == Some(1) {
        return;
    }
    let source = include_str!("../../data/ace-treasure-tables.toml");
    let value: TreasureTableSetV1 = toml::from_str(source).unwrap();
    if let Err(error) = install(value) {
        assert_eq!(active_id(), Some(1), "{error}");
    }
}
