//! Frozen source table data. Ordered rows preserve ACE ChanceTable draw behavior.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureChanceRowV1 {
    pub value: i64,
    pub probability_bits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureFloatChanceRowV1 {
    pub value_bits: u32,
    pub probability_bits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureGemRowV1 {
    pub wcid: i64,
    pub material: i64,
    pub probability_bits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureNamedValueV1 {
    pub name: String,
    pub value: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureNamedTextV1 {
    pub name: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureTableV1 {
    pub class: String,
    pub field: String,
    #[serde(default)]
    pub chance: Vec<TreasureChanceRowV1>,
    #[serde(default)]
    pub float_chance: Vec<TreasureFloatChanceRowV1>,
    #[serde(default)]
    pub gem: Vec<TreasureGemRowV1>,
    #[serde(default)]
    pub sequence: Vec<i64>,
    #[serde(default)]
    pub references: Vec<String>,
    #[serde(default)]
    pub typed_references: Vec<TreasureNamedValueV1>,
    #[serde(default)]
    pub float_sequence_bits: Vec<u32>,
    #[serde(default)]
    pub descriptors: Vec<TreasureNamedTextV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureTableSetV1 {
    pub schema_version: u16,
    pub id: u32,
    pub source_pin: String,
    pub tables: Vec<TreasureTableV1>,
    pub enums: Vec<TreasureNamedValueV1>,
    /// Spell ID to fully qualified progression-table name.
    pub spell_routes: Vec<TreasureNamedValueV1>,
    /// Fully qualified direct WCID leaf fields, including gem WCID tuples.
    pub template_leaf_tables: Vec<String>,
    pub scripts: Vec<TreasureNamedTextV1>,
}

impl TreasureTableSetV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.id == 0
            || self.source_pin.len() > 128
            || self.source_pin.is_empty()
            || self.tables.is_empty()
            || self.tables.len() > 4096
            || self.enums.len() > 131_072
            || self.spell_routes.len() > 65_536
            || self.scripts.len() > 256
            || self.template_leaf_tables.len() > 4096
        {
            return Err("treasure table-set identity/count".into());
        }
        let mut names = BTreeSet::new();
        let mut rows = 0usize;
        for t in &self.tables {
            if !valid_name(&t.class)
                || !valid_name(&t.field)
                || !names.insert(format!("{}.{}", t.class, t.field))
            {
                return Err("treasure table identity".into());
            }
            let sizes = [
                t.chance.len(),
                t.float_chance.len(),
                t.gem.len(),
                t.sequence.len(),
                t.references.len(),
                t.typed_references.len(),
                t.float_sequence_bits.len(),
                t.descriptors.len(),
            ];
            if sizes.iter().filter(|&&n| n != 0).count() != 1 || sizes.iter().any(|&n| n > 65_536) {
                return Err("treasure table shape/count".into());
            }
            rows = rows
                .checked_add(sizes.iter().sum::<usize>())
                .ok_or("treasure row overflow")?;
            if rows > 200_000
                || t.chance
                    .iter()
                    .any(|r| !valid_probability(r.probability_bits))
                || t.float_chance.iter().any(|r| {
                    !f32::from_bits(r.value_bits).is_finite()
                        || !valid_probability(r.probability_bits)
                })
                || t.gem.iter().any(|r| !valid_probability(r.probability_bits))
                || t.float_sequence_bits
                    .iter()
                    .any(|&b| !f32::from_bits(b).is_finite())
                || t.references.iter().any(|r| !valid_reference(r))
                || t.typed_references
                    .iter()
                    .any(|r| r.name == "null" || !valid_reference(&r.name))
                || t.descriptors
                    .iter()
                    .any(|r| r.text.len() > 256 || r.name.parse::<i64>().is_err())
            {
                return Err("treasure table row".into());
            }
        }
        for t in &self.tables {
            for target in t
                .references
                .iter()
                .chain(t.typed_references.iter().map(|r| &r.name))
            {
                if target != "null" && !names.contains(target) {
                    return Err(format!("missing treasure table {target}"));
                }
            }
        }
        let edges: BTreeMap<String, Vec<String>> = self
            .tables
            .iter()
            .map(|t| {
                (
                    format!("{}.{}", t.class, t.field),
                    t.references
                        .iter()
                        .filter(|r| r.as_str() != "null")
                        .cloned()
                        .chain(t.typed_references.iter().map(|r| r.name.clone()))
                        .collect(),
                )
            })
            .collect();
        fn visit(
            name: &str,
            edges: &BTreeMap<String, Vec<String>>,
            marks: &mut BTreeMap<String, u8>,
            depth: usize,
        ) -> Result<(), String> {
            if depth > 32 || marks.get(name) == Some(&1) {
                return Err("treasure table reference cycle/depth".into());
            }
            if marks.get(name) == Some(&2) {
                return Ok(());
            }
            marks.insert(name.to_owned(), 1);
            for next in &edges[name] {
                visit(next, edges, marks, depth + 1)?;
            }
            marks.insert(name.to_owned(), 2);
            Ok(())
        }
        let mut marks = BTreeMap::new();
        for name in edges.keys() {
            visit(name, &edges, &mut marks, 1)?;
        }
        let mut enums = BTreeSet::new();
        for e in &self.enums {
            if !valid_reference(&e.name) || !enums.insert(&e.name) {
                return Err(format!("treasure enum identity: {}", e.name));
            }
        }
        let mut spells = BTreeSet::new();
        for r in &self.spell_routes {
            if r.value <= 0 || !spells.insert(r.value) || !names.contains(&r.name) {
                return Err("treasure spell route".into());
            }
        }
        let mut leaves = BTreeSet::new();
        for name in &self.template_leaf_tables {
            if !names.contains(name)
                || !leaves.insert(name)
                || !self.tables.iter().any(|t| {
                    format!("{}.{}", t.class, t.field) == *name
                        && (!t.chance.is_empty() || !t.sequence.is_empty() || !t.gem.is_empty())
                })
            {
                return Err("treasure template leaf table".into());
            }
        }
        let mut scripts = BTreeSet::new();
        for s in &self.scripts {
            if s.name.len() > 256
                || s.name.is_empty()
                || !s
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
                || !scripts.insert(&s.name)
                || s.text.is_empty()
                || s.text.len() > 65_536
            {
                return Err("treasure script".into());
            }
        }
        Ok(())
    }
}

fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn valid_reference(s: &str) -> bool {
    s == "null"
        || s.split_once('.')
            .is_some_and(|(a, b)| valid_name(a) && valid_name(b))
}
fn valid_probability(bits: u32) -> bool {
    let value = f32::from_bits(bits);
    value.is_finite() && value >= 0.0
}
