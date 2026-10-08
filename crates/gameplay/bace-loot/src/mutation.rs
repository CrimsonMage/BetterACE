use crate::GraphError;
use bace_content::LootMutationV1;
use bace_gameplay_api::GeneratedItemMutation as M;
use bace_random::RandomStream;
pub(crate) fn sample(value: &LootMutationV1, stream: &mut RandomStream) -> Result<M, GraphError> {
    Ok(match value {
        LootMutationV1::Int {
            property,
            minimum,
            maximum,
        } => M::Int(
            *property,
            (i64::from(*minimum)
                + stream.below((i64::from(*maximum) - i64::from(*minimum) + 1) as u64)? as i64)
                as i32,
        ),
        LootMutationV1::Int64 {
            property,
            minimum,
            maximum,
        } => {
            let width = i128::from(*maximum) - i128::from(*minimum) + 1;
            let draw = if width == 1i128 << 64 {
                stream.next_u64()?
            } else {
                stream.below(width as u64)?
            };
            M::Int64(*property, (i128::from(*minimum) + i128::from(draw)) as i64)
        }
        LootMutationV1::Float {
            property,
            minimum,
            maximum,
        } => {
            let fraction = (stream.next_u64()? >> 11) as f64 / (1u64 << 53) as f64;
            let value = if minimum == maximum {
                *minimum
            } else {
                // Rounded interpolation can reach the upper endpoint, notably
                // when the bounds are adjacent representable floats.
                (*minimum + (*maximum - *minimum) * fraction)
                    .min(maximum.next_down())
                    .max(*minimum)
            };
            M::Float(*property, value)
        }
        LootMutationV1::DataId { property, value } => M::DataId(*property, *value),
        LootMutationV1::Bool { property, value } => M::Bool(*property, *value),
        LootMutationV1::Spell { spell } => M::Spell(*spell),
    })
}
pub(crate) fn key(value: &M) -> (u8, u32) {
    match value {
        M::Int(id, _) => (0, *id),
        M::Int64(id, _) => (1, *id),
        M::Float(id, _) => (2, *id),
        M::DataId(id, _) => (3, *id),
        M::Bool(id, _) => (4, *id),
        M::Spell(id) => (5, *id),
    }
}

#[cfg(test)]
#[path = "mutation_tests.rs"]
mod tests;
