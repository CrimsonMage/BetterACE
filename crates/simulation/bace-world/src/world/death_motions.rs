//! GDLE Monster.OnDeath: stop existing movement, then append Motion_Dead.
//! This physical lifecycle starts independently of durable corpse/reward work.
use super::*;
use bace_motion::{
    MotionDomain, MotionPlayback, MotionToken, PreparedDeathMotion, SourceMotionState,
};
use std::sync::Arc;
impl World {
    pub fn validate_death_motions(
        &self,
        actor: EntityId,
        rows: &[PreparedDeathMotion],
    ) -> Result<(), WorldError> {
        let retained = self.retained_death_style(actor, rows).map_or(0, |style| {
            self.death_motions.get(&actor).map_or(0, |old| {
                old.iter()
                    .filter(|row| {
                        row.stop
                            .source_transition()
                            .is_some_and(|s| s.before.style == style)
                    })
                    .count()
            })
        });
        if rows.is_empty()
            || rows.len() + retained > 16
            || (!self.death_motions.contains_key(&actor) && self.death_motions.len() == 4096)
        {
            return Err(WorldError::InvalidMotion);
        }
        for (index, row) in rows.iter().enumerate() {
            let stop = row
                .stop
                .source_transition()
                .ok_or(WorldError::InvalidMotion)?;
            let dead = row
                .dead
                .source_transition()
                .ok_or(WorldError::InvalidMotion)?;
            if row.stop.motion != 0x41000003
                || row.dead.motion != 0x40000011
                || stop.after != dead.before
                || dead.after.substate != 0x40000011
                || row.dead.continues_cycle()
                || row.stop.nominal_duration_seconds() + row.dead.nominal_duration_seconds() > 180.0
                || rows[..index].iter().any(|old| {
                    old.stop.source_transition().is_some_and(|s| {
                        s.before.style == stop.before.style
                            && s.before.substate == stop.before.substate
                            && s.before.speed.is_sign_negative()
                                == stop.before.speed.is_sign_negative()
                    })
                })
            {
                return Err(WorldError::InvalidMotion);
            }
        }
        Ok(())
    }
    /// Registration and equipment replacement both reserve16 rows peractor.
    pub fn register_death_motions(
        &mut self,
        actor: EntityId,
        mut rows: Vec<PreparedDeathMotion>,
    ) -> Result<(), WorldError> {
        self.body(actor)?;
        self.validate_death_motions(actor, &rows)?;
        if let Some(style) = self.retained_death_style(actor, &rows)
            && let Some(old) = self.death_motions.get(&actor)
        {
            rows.extend(
                old.iter()
                    .filter(|row| {
                        row.stop
                            .source_transition()
                            .is_some_and(|s| s.before.style == style)
                    })
                    .cloned(),
            );
        }
        self.death_motions.insert(actor, rows.into());
        Ok(())
    }
    fn retained_death_style(&self, actor: EntityId, rows: &[PreparedDeathMotion]) -> Option<u32> {
        let body = &self.actors.get(&actor)?.body;
        let style = body
            .animated_locomotion()
            .or_else(|| body.stopped_locomotion().map(|(p, _)| p))?
            .profile
            .style;
        (!rows.iter().any(|row| {
            row.stop
                .source_transition()
                .is_some_and(|s| s.before.style == style)
        }))
        .then_some(style)
    }
    pub fn has_death_motions(&self, actor: EntityId) -> bool {
        self.death_motions.contains_key(&actor)
    }
    pub fn remaining_death_motion_actors(&self) -> usize {
        4096 - self.death_motions.len()
    }
    pub fn death_motion_complete(&self, actor: EntityId, token: MotionToken, epoch: u16) -> bool {
        self.motions.get(&actor).is_some_and(|m| {
            m.epoch == epoch
                && m.playback.token == token
                && token.domain == MotionDomain::Death
                && m.playback.requested_chain().motion == 0x40000011
                && m.playback.cursor().completed
        })
    }
    pub fn begin_death_motion(&mut self, actor: EntityId) -> Result<MotionToken, WorldError> {
        let body = self.body(actor)?;
        let epoch = body.accepted().epoch();
        if self.combatants.get(&actor).is_none_or(|c| c.health() != 0)
            || self.retirement_holds.contains_key(&actor)
        {
            return Err(WorldError::LivingActor);
        }
        if let Some(current) = self
            .motions
            .get(&actor)
            .filter(|m| m.epoch == epoch && m.playback.token.domain == MotionDomain::Death)
        {
            return Ok(current.playback.token);
        }
        let before = self
            .source_motion_state(actor)
            .or_else(|| {
                body.stopped_locomotion()
                    .map(|(profile, _)| SourceMotionState {
                        style: profile.profile.style,
                        substate: 0x41000003,
                        speed: 1.0,
                    })
            })
            .ok_or(WorldError::InvalidMotion)?;
        let rows = self
            .death_motions
            .get(&actor)
            .ok_or(WorldError::InvalidMotion)?;
        let row = rows
            .iter()
            .find(|row| {
                row.stop.source_transition().is_some_and(|s| {
                    s.before.style == before.style && s.before.substate == 0x41000003
                })
            })
            .ok_or(WorldError::InvalidMotion)?;
        let current_stop = self
            .motions
            .get(&actor)
            .filter(|m| m.epoch == epoch)
            .and_then(|m| m.playback.chain().stop_chain())
            .filter(|stop| stop.source_transition().is_some_and(|s| s.before == before));
        let stored_stop = rows
            .iter()
            .find(|row| {
                row.stop.source_transition().is_some_and(|s| {
                    s.before.style == before.style
                        && s.before.substate == before.substate
                        && s.before.speed.is_sign_negative() == before.speed.is_sign_negative()
                })
            })
            .map(|row| &row.stop);
        let stop = Arc::new(
            current_stop
                .or(stored_stop)
                .ok_or(WorldError::InvalidMotion)?
                .retime_current_action(before.speed, 1.0)
                .map_err(|_| WorldError::InvalidMotion)?,
        );
        let dead = row.dead.clone();
        let owner = self
            .motion_last
            .get(&(actor, MotionDomain::Death))
            .map_or(Some(1), |t| t.0.checked_add(1))
            .ok_or(WorldError::InvalidMotion)?;
        let stopping = MotionToken {
            domain: MotionDomain::Death,
            owner,
            sequence: 1,
        };
        let token = MotionToken {
            sequence: 2,
            ..stopping
        };
        let playback = if let Some(current) = self.motions.get(&actor).filter(|m| m.epoch == epoch)
        {
            current
                .playback
                .append_stop(stop, stopping)
                .and_then(|p| p.append_stop(dead, token))
        } else if before.substate == 0x41000003 {
            MotionPlayback::new(dead, token)
        } else {
            MotionPlayback::new(stop, stopping).and_then(|p| p.append_stop(dead, token))
        }
        .map_err(|_| WorldError::InvalidMotion)?;
        if self.motions.len() >= 4096 && !self.motions.contains_key(&actor)
            || self.motion_last.len() >= 12288
                && !self.motion_last.contains_key(&(actor, MotionDomain::Death))
        {
            return Err(WorldError::InvalidMotion);
        }
        self.reserve_motion_buffers();
        self.actors
            .get_mut(&actor)
            .expect("preflighted dead body")
            .body
            .stop_motion();
        self.motion_last
            .insert((actor, MotionDomain::Death), (owner, 2));
        self.motions.insert(
            actor,
            motions::OwnedMotion {
                playback,
                epoch,
                ending: false,
            },
        );
        Ok(token)
    }
    pub(super) fn start_pending_death_motions(&mut self) {
        let mut actors = [EntityId(0); 256];
        let mut count = 0;
        for wrapped in [false, true] {
            for &actor in self.death_motions.keys() {
                if self.death_motion_cursor.is_some_and(|old| actor <= old) != wrapped {
                    continue;
                }
                if self.combatants.get(&actor).is_some_and(|c| c.health() == 0)
                    && self.actors.get(&actor).is_some_and(|a| {
                        !self.motions.get(&actor).is_some_and(|m| {
                            m.epoch == a.body.accepted().epoch()
                                && m.playback.token.domain == MotionDomain::Death
                        })
                    })
                {
                    actors[count] = actor;
                    count += 1;
                    if count == actors.len() {
                        break;
                    }
                }
            }
            if count == actors.len() {
                break;
            }
        }
        for actor in actors.into_iter().take(count) {
            self.death_motion_cursor = Some(actor);
            if self.begin_death_motion(actor).is_err() {
                self.motion_blocked.insert(actor);
            }
        }
    }
}
