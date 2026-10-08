//! Change combat style through the actual GDLE motion-table links before starting
//! the cast driver. Style callbacks share the cast owner, with reserved tokens1/2.
use super::*;
pub(super) struct StyleEntry {
    pub completion: Option<bool>,
    pub epoch: u16,
    deadline: f64,
    preparation: CastPreparation,
}
impl Magic {
    pub(super) fn begin_style_entry(
        &mut self,
        attempt: &mut Attempt,
        chain: Arc<bace_motion::PreparedMotionChain>,
        preparation: CastPreparation,
        world: &mut World,
        now: f64,
    ) -> Result<(), CastRejection> {
        let actor = attempt.origin.actor();
        let epoch = world
            .actor_state(actor)
            .map_err(|_| CastRejection::MissingActor)?
            .1
            .epoch();
        let token = bace_motion::MotionToken {
            domain: bace_motion::MotionDomain::Casting,
            owner: attempt.cast,
            sequence: 1,
        };
        world
            .begin_motion(actor, token, chain.clone())
            .map_err(|_| CastRejection::MissingAssets)?;
        attempt.motion_sequence_offset = 2;
        attempt.style_entry = Some(StyleEntry {
            completion: None,
            epoch,
            deadline: now + 15.0,
            preparation,
        });
        self.events.push_back(MagicEvent::Motion {
            actor,
            cast: attempt.cast,
            sequence: 1,
            motion: chain.motion,
            speed: chain.speed,
        });
        Ok(())
    }
    /// False retains the complete attempt without spending resources or advancing
    /// gesture recovery while the style links still own the World cursor.
    pub(super) fn advance_style_entry(
        &mut self,
        attempt: &mut Attempt,
        world: &mut World,
        now: f64,
    ) -> Result<bool, CastRejection> {
        let Some(entry) = &attempt.style_entry else {
            return Ok(true);
        };
        let actor = attempt.origin.actor();
        if now > entry.deadline
            || !world
                .actor_state(actor)
                .is_ok_and(|(_, s)| s.epoch() == entry.epoch)
        {
            return Err(CastRejection::InvalidState);
        }
        match entry.completion {
            None => return Ok(false),
            Some(false) => return Err(CastRejection::InvalidState),
            Some(true) => {}
        }
        // Keep all source links. Ready gets sequence2; first gesture gets3.
        world
            .end_cast_motion(actor, attempt.cast)
            .map_err(|_| CastRejection::InvalidState)?;
        let skill = attempt.cast_skill;
        let mut observation = observe(
            world,
            actor,
            self.observed_target(attempt, world)?,
            &attempt.prepared.spell,
            skill,
        )?;
        if !matches!(attempt.origin, CastOrigin::Player(_)) {
            observation.peace_mode = false;
        }
        let entry = attempt.style_entry.take().expect("checked style");
        let signal = attempt
            .driver
            .begin(entry.preparation, now, observation)
            .map_err(rejection)?;
        let (cell, accepted) = world
            .actor_state(actor)
            .map_err(|_| CastRejection::MissingActor)?;
        let body = world.body(actor).map_err(|_| CastRejection::MissingActor)?;
        attempt.initial_cast = (
            cell,
            accepted.position()
                - if body.collision_shape().is_none() {
                    Vec3::new(0.0, 0.0, body.collision_radius())
                } else {
                    Vec3::ZERO
                },
        );
        self.signal(attempt, signal, world, now)?;
        Ok(true)
    }
}
