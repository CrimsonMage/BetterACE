/// Generated item property proposals, applied by the item owner before durable
/// publication. Ownership/placement and account privilege fields are not inputs.
#[derive(Clone, Debug, PartialEq)]
pub enum GeneratedItemMutation {
    Int(u32, i32),
    Int64(u32, i64),
    Float(u32, f64),
    DataId(u32, u32),
    Bool(u32, bool),
    Spell(u32),
}
