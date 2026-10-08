//! Bounded preparation and pure evaluation of pinned ACE weapon mutation scripts.
//! Native authoring remains TOML; these are imported upstream compatibility inputs.
mod evaluator;
mod parser;
use crate::{TreasureError, TreasureRandom};
use bace_content::WeenieV1;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct MutationScripts {
    scripts: BTreeMap<String, MutationScript>,
}
impl MutationScripts {
    /// Prepare once off the simulation thread. No I/O or lazy cache is involved.
    pub fn pinned() -> Result<Self, TreasureError> {
        let mut scripts = BTreeMap::new();
        let sources = crate::ace_tables::script_sources()
            .ok_or_else(|| TreasureError::MissingSourceTable("accepted mutation scripts".into()))?;
        for (name, source) in sources {
            scripts.insert(name.replace('/', "."), MutationScript::parse(source)?);
        }
        Ok(Self { scripts })
    }
    pub fn len(&self) -> usize {
        self.scripts.len()
    }
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.scripts.keys().map(String::as_str)
    }
    pub fn apply<R: TreasureRandom>(
        &self,
        name: &str,
        item: &mut WeenieV1,
        tier: i32,
        random: &mut R,
    ) -> Result<bool, TreasureError> {
        if name.len() > 256 {
            return Err(TreasureError::Bounds);
        }
        self.scripts
            .get(&name.replace('/', "."))
            .ok_or_else(|| TreasureError::MissingSourceTable(name.to_owned()))?
            .apply(item, tier, random)
    }
}
#[derive(Clone, Debug)]
pub struct MutationScript {
    mutations: Vec<Mutation>,
}
#[derive(Clone, Debug)]
struct Mutation {
    chances: Vec<f32>,
    outcomes: Vec<Vec<EffectList>>,
}
#[derive(Clone, Debug)]
struct EffectList {
    chance: f32,
    effects: Vec<Effect>,
}
#[derive(Clone, Copy, Debug)]
enum Quality {
    Int(u32),
    Float(u32),
}
#[derive(Clone, Copy, Debug)]
enum Argument {
    Int(i32),
    Float(f64),
    Quality(Quality),
    Random(f32, f32),
}
#[derive(Clone, Copy, Debug)]
enum Operation {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    AtLeastAdd,
    AtMostSubtract,
    AddMultiply,
    AddDivide,
    SubtractMultiply,
    SubtractDivide,
    AssignAdd,
    AssignSubtract,
    AssignMultiply,
    AssignDivide,
}
#[derive(Clone, Copy, Debug)]
struct Effect {
    quality: Quality,
    operation: Operation,
    arg1: Argument,
    arg2: Option<Argument>,
}
