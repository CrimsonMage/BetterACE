//! GDLE MonsterAI.cpp RollDiceCastSpell: independent spellbook draws, first
//! successful entry, AI_USE_MAGIC_DELAY before CreatureBeginCast. Explicit time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonsterSpell {
    pub spell: u32,
    pub likelihood: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonsterCastError {
    InvalidProfile,
    InvalidTime,
    InvalidDraws,
}
#[derive(Clone, Debug)]
pub struct MonsterSpellcasting {
    entries: Vec<MonsterSpell>,
    delay: f64,
    next_cast: f64,
    last_time: f64,
}
impl MonsterSpellcasting {
    pub fn new(entries: Vec<MonsterSpell>, delay: f64) -> Result<Self, MonsterCastError> {
        if entries.len() > 256
            || !delay.is_finite()
            || !(0.0..=86400.0).contains(&delay)
            || entries.iter().any(|s| {
                s.spell == 0 || !s.likelihood.is_finite() || !(0.0..=1.0).contains(&s.likelihood)
            })
        {
            return Err(MonsterCastError::InvalidProfile);
        }
        let mut ids = std::collections::BTreeSet::new();
        if entries.iter().any(|s| !ids.insert(s.spell)) {
            return Err(MonsterCastError::InvalidProfile);
        }
        Ok(Self {
            entries,
            delay,
            next_cast: 0.0,
            last_time: 0.0,
        })
    }
    pub fn entries(&self) -> &[MonsterSpell] {
        &self.entries
    }
    pub fn ready(&self, now: f64) -> bool {
        now.is_finite() && now >= self.last_time && now >= self.next_cast
    }
    /// Draws are in authoritative prepared spellbook iteration order, not a
    /// single normalized weighted draw. Caller supplies a fresh event stream.
    pub fn select(&mut self, now: f64, draws: &[f32]) -> Result<Option<u32>, MonsterCastError> {
        if !now.is_finite() || now < self.last_time {
            return Err(MonsterCastError::InvalidTime);
        }
        if draws.len() != self.entries.len()
            || draws
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(MonsterCastError::InvalidDraws);
        }
        if now < self.next_cast {
            return Ok(None);
        }
        let selected = self
            .entries
            .iter()
            .zip(draws)
            .find(|(spell, draw)| **draw <= spell.likelihood)
            .map(|(spell, _)| spell.spell);
        if selected.is_some() {
            self.next_cast = now + self.delay;
        }
        self.last_time = now;
        Ok(selected)
    }
}
