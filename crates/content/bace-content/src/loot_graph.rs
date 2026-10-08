//! Frozen native nested-table graph. Selection mode is part of the data, not an
//! implicit normalization of legacy cumulative probabilities.
use crate::{LootMutationV1, ProbabilityV1};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LootSelectionV1 {
    All,
    Weighted,
    Independent,
    OrderedCumulative,
    Item,
    Nothing,
    Mutation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LootItemV1 {
    pub template: u32,
    pub minimum_stack: u32,
    pub maximum_stack: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LootBranchV1 {
    pub id: String,
    pub target: String,
    pub minimum_rolls: u32,
    pub maximum_rolls: u32,
    pub weight: Option<u64>,
    pub probability: Option<ProbabilityV1>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LootNodeV1 {
    pub mutation: Option<LootMutationV1>,
    pub id: String,
    pub selection: LootSelectionV1,
    pub item: Option<LootItemV1>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub branches: Vec<LootBranchV1>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LootGraphV1 {
    pub schema_version: u16,
    pub id: u32,
    pub root: String,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub nodes: Vec<LootNodeV1>,
}
fn name(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 64
}
impl LootGraphV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.id == 0
            || self.nodes.is_empty()
            || self.nodes.len() > 4096
        {
            return Err("loot graph identity/count".into());
        }
        let mut nodes = BTreeMap::new();
        let mut edges = 0usize;
        for (index, node) in self.nodes.iter().enumerate() {
            if !name(&node.id)
                || node.id == "$rare"
                || nodes.insert(node.id.as_str(), index).is_some()
            {
                return Err("loot node identity".into());
            }
            edges = edges
                .checked_add(node.branches.len())
                .ok_or("loot edge count")?;
            if node.branches.len() > 1024 || edges > 16384 {
                return Err("loot edge limit".into());
            }
            if node.selection != LootSelectionV1::Mutation && node.mutation.is_some() {
                return Err("unexpected loot mutation".into());
            }
            match node.selection {
                LootSelectionV1::Item => {
                    let item = node.item.as_ref().ok_or("item node requires template")?;
                    if item.template == 0
                        || item.minimum_stack == 0
                        || item.minimum_stack > item.maximum_stack
                        || item.maximum_stack > 1_000_000
                    {
                        return Err("loot item bounds".into());
                    }
                }
                LootSelectionV1::Mutation => {
                    if node.item.is_some() || !node.branches.is_empty() {
                        return Err("mutation node shape".into());
                    }
                    node.mutation
                        .as_ref()
                        .ok_or("missing loot mutation")?
                        .validate()?;
                }
                LootSelectionV1::Nothing => {
                    if node.item.is_some() || !node.branches.is_empty() {
                        return Err("empty node has content".into());
                    }
                }
                _ => {
                    if node.item.is_some() || node.branches.is_empty() {
                        return Err("table node shape".into());
                    }
                }
            }
            let mut ids = BTreeSet::new();
            let mut weight = 0u64;
            let mut cumulative = 0u64;
            let mut denominator = None;
            for branch in &node.branches {
                if !name(&branch.id)
                    || !name(&branch.target)
                    || !ids.insert(&branch.id)
                    || branch.minimum_rolls > branch.maximum_rolls
                    || branch.maximum_rolls > 256
                {
                    return Err("loot branch identity/count".into());
                }
                match node.selection {
                    LootSelectionV1::Weighted => {
                        if branch.probability.is_some() {
                            return Err("weighted branch cannot have probability".into());
                        }
                        weight = weight
                            .checked_add(branch.weight.ok_or("missing branch weight")?)
                            .ok_or("loot weight overflow")?;
                    }
                    LootSelectionV1::Independent | LootSelectionV1::OrderedCumulative => {
                        if branch.weight.is_some() {
                            return Err("probability branch cannot have weight".into());
                        }
                        let p = branch.probability.ok_or("missing branch probability")?;
                        p.validate()?;
                        if node.selection == LootSelectionV1::OrderedCumulative {
                            if denominator.is_some_and(|d| d != p.denominator) {
                                return Err("cumulative probabilities need one denominator".into());
                            }
                            denominator = Some(p.denominator);
                            cumulative = cumulative
                                .checked_add(p.numerator)
                                .ok_or("probability sum overflow")?;
                            if cumulative > p.denominator {
                                return Err("cumulative probability exceeds one".into());
                            }
                        }
                    }
                    _ => {
                        if branch.weight.is_some() || branch.probability.is_some() {
                            return Err("unexpected branch probability".into());
                        }
                    }
                }
            }
            if node.selection == LootSelectionV1::Weighted && weight == 0 {
                return Err("empty weight distribution".into());
            }
        }
        let root = *nodes.get(self.root.as_str()).ok_or("missing loot root")?;
        for node in &self.nodes {
            for branch in &node.branches {
                if !nodes.contains_key(branch.target.as_str()) {
                    return Err("missing loot child".into());
                }
            }
        }
        // Validate every node, including currently unreachable authoring nodes.
        let mut marks = vec![0u8; self.nodes.len()];
        let mut depths = vec![0usize; self.nodes.len()];
        fn visit(
            i: usize,
            g: &LootGraphV1,
            keys: &BTreeMap<&str, usize>,
            marks: &mut [u8],
            depths: &mut [usize],
            depth: usize,
        ) -> Result<usize, String> {
            if depth > 32 || marks[i] == 1 {
                return Err("loot cycle/depth limit".into());
            }
            if marks[i] == 2 {
                return Ok(depths[i]);
            }
            marks[i] = 1;
            let mut maximum = 1;
            for b in &g.nodes[i].branches {
                maximum = maximum
                    .max(1 + visit(keys[b.target.as_str()], g, keys, marks, depths, depth + 1)?);
            }
            if maximum > 32 {
                return Err("loot depth limit".into());
            }
            marks[i] = 2;
            depths[i] = maximum;
            Ok(maximum)
        }
        visit(root, self, &nodes, &mut marks, &mut depths, 1)?;
        for i in 0..self.nodes.len() {
            visit(i, self, &nodes, &mut marks, &mut depths, 1)?;
        }
        fn contexts(
            i: usize,
            inside: bool,
            g: &LootGraphV1,
            keys: &BTreeMap<&str, usize>,
            seen: &mut BTreeSet<(usize, bool)>,
        ) -> Result<(), String> {
            if !seen.insert((i, inside)) {
                return Ok(());
            }
            let node = &g.nodes[i];
            if (node.selection == LootSelectionV1::Mutation && !inside)
                || (node.selection == LootSelectionV1::Item && inside)
            {
                return Err("loot item/mutation context mismatch".into());
            }
            let inside = inside || node.selection == LootSelectionV1::Item;
            for branch in &node.branches {
                contexts(keys[branch.target.as_str()], inside, g, keys, seen)?;
            }
            Ok(())
        }
        contexts(root, false, self, &nodes, &mut BTreeSet::new())?;
        Ok(())
    }
}
