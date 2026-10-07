use crate::QuestProgress;

/// ACE HasQuestSolves treats a missing registry entry as zero solves and uses
/// inclusive bounds. Signed counters are retained for quest-bit compatibility.
pub fn has_solves(
    progress: Option<&QuestProgress>,
    minimum: Option<i32>,
    maximum: Option<i32>,
) -> bool {
    let solves = progress.map_or(0, |record| record.completions);
    solves >= minimum.unwrap_or(i32::MIN) && solves <= maximum.unwrap_or(i32::MAX)
}

/// Missing quest is false even for a zero bit mask, matching ACE.
pub fn has_bits(progress: Option<&QuestProgress>, bits: i32) -> bool {
    progress.is_some_and(|record| record.completions & bits == bits)
}

/// Missing quest has no bits, including when the queried mask is zero.
pub fn has_no_bits(progress: Option<&QuestProgress>, bits: i32) -> bool {
    progress.is_none_or(|record| record.completions & bits == 0)
}
