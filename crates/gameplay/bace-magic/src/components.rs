//! ACE SpellFormula.GetFociFormula/ScarabPower. Component occurrences are retained
//! because each occurrence gets an independent destruction draw.
use crate::MagicFormulaError;
pub fn foci_formula(base: &[u32]) -> Result<Vec<u32>, MagicFormulaError> {
    if base.len() > 8 {
        return Err(MagicFormulaError::InvalidInput);
    }
    let power = match base.first().copied().unwrap_or(0) {
        n @ 1..=6 => n,
        110 => 7,
        112 => 8,
        192 => 9,
        193 => 10,
        _ => 0,
    };
    let tapers = match power {
        1 => 1,
        2 => 2,
        3 | 4 | 7 => 3,
        5 | 6 | 8 | 9 | 10 => 4,
        _ => 0,
    };
    let mut result: Vec<_> = base
        .iter()
        .copied()
        .filter(|c| matches!(c, 1..=6 | 110 | 111 | 112 | 192 | 193))
        .collect();
    result.extend(std::iter::repeat_n(188, tapers));
    Ok(result)
}
