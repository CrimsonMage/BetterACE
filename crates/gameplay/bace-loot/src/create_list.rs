//! ACE Creature_Equipment.CreateListSelect, official 47edade3, AGPL-3.0-only.
//! Default trophy_drop_rate = 1 only. Rows and selected WCID zero placeholders
//! retain source order; the instantiation owner handles the no-item placeholder.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CreateEntry {
    pub template: u32,
    pub destination: u32,
    pub shade: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LootError {
    Capacity,
    InvalidProbability,
    InvalidRandom,
    MissingRandom,
    OutputCapacity,
}

/// Select row indices using explicit [0,1) draws. On failure output is unchanged.
/// Returns draws consumed, including ACE's unconditional initial draw. Caller
/// retains RNG ownership and advances it only after this operation succeeds.
pub fn select_create_list(
    entries: &[CreateEntry],
    draws: &[f32],
    output: &mut Vec<usize>,
) -> Result<usize, LootError> {
    if entries.len() > 4096 {
        return Err(LootError::Capacity);
    }
    for entry in entries {
        if !entry.shade.is_finite() || !(0.0..=1.0).contains(&entry.shade) {
            return Err(LootError::InvalidProbability);
        }
    }
    let mut draw_count = 1;
    let mut total = 0.0_f32;
    for entry in entries {
        if entry.destination & 8 != 0 && entry.shade != 0.0 {
            if total >= 1.0 {
                total = 0.0;
                draw_count += 1;
            }
            total += entry.shade;
        }
    }
    if draws.len() < draw_count {
        return Err(LootError::MissingRandom);
    }
    if draws[..draw_count]
        .iter()
        .any(|d| !d.is_finite() || !(0.0..1.0).contains(d))
    {
        return Err(LootError::InvalidRandom);
    }
    // Count before appending so capacity rejection cannot partially publish loot.
    let walk = |visit: &mut dyn FnMut(usize)| {
        let mut draw_index = 0;
        let mut probability = 0.0_f32;
        let mut selected = false;
        for (index, entry) in entries.iter().enumerate() {
            if entry.destination & 8 != 0 && entry.shade != 0.0 {
                if probability >= 1.0 {
                    probability = 0.0;
                    draw_index += 1;
                    selected = false;
                }
                probability += entry.shade;
                if selected || draws[draw_index] >= probability {
                    continue;
                }
                selected = true;
            }
            visit(index);
        }
    };
    let mut count = 0;
    walk(&mut |_| count += 1);
    if output.capacity() - output.len() < count {
        return Err(LootError::OutputCapacity);
    }
    walk(&mut |index| output.push(index));
    Ok(draw_count)
}
