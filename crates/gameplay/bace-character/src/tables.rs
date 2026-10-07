/// Validated cumulative XP thresholds supplied by prepared, immutable assets.
/// No synthetic thresholds are substituted when DAT data is unavailable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankTable {
    thresholds: Box<[u32]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RankTableError {
    Empty,
    TooManyRanks,
    MissingZeroRank,
    DecreasingThresholds,
}

impl RankTable {
    /// ACE stores purchased ranks as ushort. Repeated thresholds are legal:
    /// its reverse scan selects the highest rank affordable at that threshold.
    pub fn new(thresholds: &[u32]) -> Result<Self, RankTableError> {
        if thresholds.is_empty() {
            return Err(RankTableError::Empty);
        }
        if thresholds.len() > usize::from(u16::MAX) + 1 {
            return Err(RankTableError::TooManyRanks);
        }
        if thresholds[0] != 0 {
            return Err(RankTableError::MissingZeroRank);
        }
        if thresholds.windows(2).any(|pair| pair[0] > pair[1]) {
            return Err(RankTableError::DecreasingThresholds);
        }
        Ok(Self {
            thresholds: thresholds.into(),
        })
    }

    /// Equivalent to ACE Player.CalcAttributeRank/CalcVitalRank/CalcSkillRank
    /// on a validated cumulative table; logarithmic work, no tick allocation.
    pub fn rank(&self, experience: u32) -> u16 {
        (self.thresholds.partition_point(|xp| *xp <= experience) - 1) as u16
    }

    pub fn maximum_rank(&self) -> u16 {
        (self.thresholds.len() - 1) as u16
    }

    pub fn maximum_experience(&self) -> u32 {
        self.thresholds[self.thresholds.len() - 1]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgressionTables {
    pub attributes: RankTable,
    pub vitals: RankTable,
    pub trained_skills: RankTable,
    pub specialized_skills: RankTable,
}
