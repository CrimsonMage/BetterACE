//! Original Creature_Equipment.CreateListSelect default trophy-rate overload.
//! ACE compares a double draw against a single-precision cumulative probability.
use crate::{TreasureError, TreasureRandom, treasure_random::unit};
use bace_content::CreateListEntry;
pub fn generate_create_list_selection<R: TreasureRandom>(
    rows: &[CreateListEntry],
    random: &mut R,
) -> Result<Vec<usize>, TreasureError> {
    if rows.len() > 4096 {
        return Err(TreasureError::Capacity);
    }
    let mut cursor = random.clone();
    let mut roll = unit(&mut cursor)?;
    let mut probability = 0.0f32;
    let mut selected = false;
    let mut output = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if !row.shade.is_finite() {
            return Err(TreasureError::Bounds);
        }
        if row.destination_type & 8 != 0 && row.shade != 0.0 {
            if row.shade < 0.0 {
                return Err(TreasureError::Bounds);
            }
            if probability >= 1.0 {
                probability = 0.0;
                roll = unit(&mut cursor)?;
                selected = false;
            }
            probability += row.shade;
            if !probability.is_finite() {
                return Err(TreasureError::Bounds);
            }
            if selected || roll >= f64::from(probability) {
                continue;
            }
            selected = true;
        }
        output.push(index);
    }
    *random = cursor;
    Ok(output)
}
