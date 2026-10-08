//! Single-owner health and prepared melee inputs. These are runtime state, not
//! frozen save DTOs. Prepared damage/hooks do not establish full ACE formulas.

#[derive(Clone, Debug, PartialEq)]
pub struct CombatantProfile {
    pub maximum_health: u32,
    pub melee_damage: u32,
    pub melee_range: f32,
    pub attack_duration: f64,
    pub strike_offsets: Vec<f64>,
    pub player: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatantError {
    InvalidProfile,
    RevisionOverflow,
}
#[derive(Debug)]
pub struct Combatant {
    profile: CombatantProfile,
    health: u32,
    revision: u64,
    /// Immutable admitted player generation for queued private output. This is
    /// a lifetime stamp, never an authorization or mutable session registry.
    incarnation: u64,
    mode: u32,
    lifestone_protected: bool,
    death_pk_status: Option<u32>,
    contributors: Vec<(bace_types::EntityId, f32)>,
    stamina: Option<crate::VitalPool>,
    mana: Option<crate::VitalPool>,
    /// Ephemeral player PK activity, in the explicit simulation clock.
    pk_activity_until: f64,
}
impl Combatant {
    pub fn new(profile: CombatantProfile) -> Result<Self, CombatantError> {
        let melee_disabled = profile.melee_damage == 0
            && profile.melee_range == 0.0
            && profile.attack_duration == 0.0
            && profile.strike_offsets.is_empty();
        if profile.maximum_health == 0
            || (!melee_disabled
                && (!profile.melee_range.is_finite()
                    || profile.melee_range <= 0.0
                    || !profile.attack_duration.is_finite()
                    || profile.attack_duration <= 0.0
                    || profile.strike_offsets.is_empty()
                    || profile.strike_offsets.len() > 32
                    || profile
                        .strike_offsets
                        .iter()
                        .any(|t| !t.is_finite() || *t < 0.0 || *t > profile.attack_duration)
                    || profile.strike_offsets.windows(2).any(|t| t[0] > t[1])))
        {
            return Err(CombatantError::InvalidProfile);
        }
        Ok(Self {
            health: profile.maximum_health,
            profile,
            revision: 0,
            incarnation: 0,
            mode: 1,
            lifestone_protected: false,
            death_pk_status: None,
            contributors: Vec::with_capacity(256),
            stamina: None,
            mana: None,
            pk_activity_until: 0.,
        })
    }

    pub fn incarnation(&self) -> u64 {
        self.incarnation
    }
    pub fn validate_incarnation(&self, generation: u64) -> Result<(), CombatantError> {
        if !self.profile.player
            || generation == 0
            || (self.incarnation != 0 && self.incarnation != generation)
        {
            Err(CombatantError::InvalidProfile)
        } else {
            Ok(())
        }
    }
    pub fn stamp_incarnation(&mut self, generation: u64) -> Result<(), CombatantError> {
        self.validate_incarnation(generation)?;
        self.incarnation = generation;
        Ok(())
    }
    /// The gameplay owner supplies its source-derived deadline. This runtime
    /// gate is independent of health revisions and contains no wall-clock reads.
    pub fn set_pk_activity_deadline(&mut self, deadline: f64) -> Result<(), CombatantError> {
        if !self.profile.player || !deadline.is_finite() || deadline < 0. {
            return Err(CombatantError::InvalidProfile);
        }
        self.pk_activity_until = self.pk_activity_until.max(deadline);
        Ok(())
    }
    pub fn pk_activity_active(&self, now: f64) -> bool {
        now.is_finite() && now < self.pk_activity_until
    }
    pub fn clear_pk_activity(&mut self) {
        self.pk_activity_until = 0.;
    }
    pub fn with_resources(
        mut self,
        stamina: Option<crate::VitalPool>,
        mana: Option<crate::VitalPool>,
    ) -> Result<Self, CombatantError> {
        if [stamina, mana]
            .into_iter()
            .flatten()
            .any(|v| v.current > v.maximum || v.maximum > i32::MAX as u32)
        {
            return Err(CombatantError::InvalidProfile);
        }
        self.stamina = stamina;
        self.mana = mana;
        Ok(self)
    }
    pub fn vital(&self, kind: crate::EntityVital) -> Option<crate::VitalPool> {
        match kind {
            crate::EntityVital::Health => Some(crate::VitalPool {
                current: self.health,
                maximum: self.profile.maximum_health,
            }),
            crate::EntityVital::Stamina => self.stamina,
            crate::EntityVital::Mana => self.mana,
        }
    }
    pub fn validate_vital_maxima(&self, maxima: [u32; 3]) -> Result<(), CombatantError> {
        if maxima.iter().any(|v| *v == 0 || *v > i32::MAX as u32)
            || self.stamina.is_none()
            || self.mana.is_none()
        {
            return Err(CombatantError::InvalidProfile);
        }
        if self.revision == u64::MAX {
            return Err(CombatantError::RevisionOverflow);
        }
        Ok(())
    }
    /// Source vital maximum changes preserve current values up to the new cap;
    /// they never heal, refill or revive a player as a side effect of a buff.
    pub fn replace_vital_maxima(&mut self, maxima: [u32; 3]) -> Result<(), CombatantError> {
        self.validate_vital_maxima(maxima)?;
        let stamina = self.stamina.expect("validated stamina");
        let mana = self.mana.expect("validated mana");
        if [self.profile.maximum_health, stamina.maximum, mana.maximum] == maxima {
            return Ok(());
        }
        self.profile.maximum_health = maxima[0];
        self.health = self.health.min(maxima[0]);
        self.stamina = Some(crate::VitalPool {
            current: stamina.current.min(maxima[1]),
            maximum: maxima[1],
        });
        self.mana = Some(crate::VitalPool {
            current: mana.current.min(maxima[2]),
            maximum: maxima[2],
        });
        self.revision += 1;
        Ok(())
    }
    pub fn can_record_damage(&self, actor: bace_types::EntityId) -> bool {
        self.contributors.len() < 256 || self.contributors.iter().any(|(id, _)| *id == actor)
    }
    pub fn apply_vital(
        &mut self,
        kind: crate::EntityVital,
        before: u32,
        after: u32,
        damage_source: Option<bace_types::EntityId>,
    ) -> Result<(), CombatantError> {
        let pool = self.vital(kind).ok_or(CombatantError::InvalidProfile)?;
        if pool.current != before || after > pool.maximum {
            return Err(CombatantError::InvalidProfile);
        }
        if before == after {
            return Ok(());
        }
        if kind == crate::EntityVital::Health
            && after < before
            && let Some(source) = damage_source
        {
            self.damage_from(source, before - after)?;
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(CombatantError::RevisionOverflow)?;
        match kind {
            crate::EntityVital::Health => self.health = after,
            crate::EntityVital::Stamina => {
                self.stamina = Some(crate::VitalPool {
                    current: after,
                    ..pool
                })
            }
            crate::EntityVital::Mana => {
                self.mana = Some(crate::VitalPool {
                    current: after,
                    ..pool
                })
            }
        }
        self.revision = revision;
        Ok(())
    }
    pub fn profile(&self) -> &CombatantProfile {
        &self.profile
    }
    pub fn health(&self) -> u32 {
        self.health
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn mode(&self) -> u32 {
        self.mode
    }
    pub fn set_mode(&mut self, mode: u32) -> bool {
        if mode == 1 || mode == 2 || mode == 4 || mode == 8 {
            self.mode = mode;
            true
        } else {
            false
        }
    }
    pub fn damage(&mut self, amount: u32) -> Result<u32, CombatantError> {
        let amount = if self.lifestone_protected {
            0
        } else {
            amount.min(self.health)
        };
        if amount == 0 {
            return Ok(0);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(CombatantError::RevisionOverflow)?;
        self.health -= amount;
        self.revision = revision;
        Ok(amount)
    }
    pub fn death_pk_status(&self) -> Option<u32> {
        self.death_pk_status
    }
    pub fn set_death_pk_status(&mut self, value: u32) {
        self.death_pk_status = Some(value);
    }
    pub fn lifestone_protected(&self) -> bool {
        self.lifestone_protected
    }
    pub fn set_lifestone_protected(&mut self, value: bool) {
        self.lifestone_protected = value;
    }
    pub fn restore_after_death(&mut self, maxima: [u32; 3]) -> Result<(), CombatantError> {
        if !self.profile.player
            || self.health != 0
            || maxima.iter().any(|v| *v == 0 || *v > i32::MAX as u32)
        {
            return Err(CombatantError::InvalidProfile);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(CombatantError::RevisionOverflow)?;
        let current = |v: u32| (v as f32 * 0.75).round_ties_even() as u32;
        self.profile.maximum_health = maxima[0];
        self.health = current(maxima[0]);
        self.stamina = Some(crate::VitalPool {
            current: current(maxima[1]),
            maximum: maxima[1],
        });
        self.mana = Some(crate::VitalPool {
            current: current(maxima[2]),
            maximum: maxima[2],
        });
        self.mode = 1;
        self.contributors.clear();
        self.revision = revision;
        Ok(())
    }
    pub fn reset_damage_history(&mut self) {
        self.contributors.clear();
    }
    pub fn contributors(&self) -> &[(bace_types::EntityId, f32)] {
        &self.contributors
    }
    pub fn damage_from(
        &mut self,
        actor: bace_types::EntityId,
        amount: u32,
    ) -> Result<u32, CombatantError> {
        let index = self.contributors.iter().position(|(id, _)| *id == actor);
        if index.is_none() && self.contributors.len() == 256 {
            return Err(CombatantError::InvalidProfile);
        }
        let applied = self.damage(amount)?;
        if applied != 0 {
            if let Some(index) = index {
                self.contributors[index].1 += applied as f32;
            } else {
                self.contributors.push((actor, applied as f32));
            }
        }
        Ok(applied)
    }
}
