use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LootMutationV1 {
    Int {
        property: u32,
        minimum: i32,
        maximum: i32,
    },
    Int64 {
        property: u32,
        minimum: i64,
        maximum: i64,
    },
    Float {
        property: u32,
        minimum: f64,
        maximum: f64,
    },
    DataId {
        property: u32,
        value: u32,
    },
    Bool {
        property: u32,
        value: bool,
    },
    Spell {
        spell: u32,
    },
}
impl LootMutationV1 {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Int {
                minimum, maximum, ..
            } if minimum > maximum => Err("loot integer range".into()),
            Self::Int64 {
                minimum, maximum, ..
            } if minimum > maximum => Err("loot int64 range".into()),
            Self::Float {
                minimum, maximum, ..
            } if !minimum.is_finite()
                || !maximum.is_finite()
                || minimum > maximum
                || !(maximum - minimum).is_finite() =>
            {
                Err("loot float range".into())
            }
            Self::Spell { spell } if *spell == 0 || *spell > u32::from(u16::MAX) => {
                Err("loot spell identity".into())
            }
            _ => Ok(()),
        }
    }
}
