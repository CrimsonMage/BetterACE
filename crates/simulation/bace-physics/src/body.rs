mod server_move;
use crate::SyntheticScene;
use bace_geometry::Vec3;
use bace_motion::{Capabilities, MotionError, MotionIntent, TurnControl, TurnIntent};

pub const STEP_SECONDS: f32 = 1.0 / 30.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcceptedState {
    position: Vec3,
    velocity: Vec3,
    grounded: bool,
    epoch: u16,
    heading: f32,
}

impl AcceptedState {
    pub fn position(self) -> Vec3 {
        self.position
    }
    pub fn velocity(self) -> Vec3 {
        self.velocity
    }
    pub fn grounded(self) -> bool {
        self.grounded
    }
    pub fn epoch(self) -> u16 {
        self.epoch
    }
    pub fn heading_radians(self) -> f32 {
        self.heading
    }
}

pub struct Body {
    accepted: AcceptedState,
    geometry_shape: Option<std::sync::Arc<crate::CollisionShape>>,
    animated: Option<std::sync::Arc<bace_motion::AnimatedLocomotion>>,
    stopped_locomotion: Option<(std::sync::Arc<bace_motion::AnimatedLocomotion>, f32)>,
    root_cursor: bace_motion::RootCursor,
    motion_root: Option<bace_motion::RootFrame>,
    locomotion: Option<bace_motion::MotionDrive>,
    locomotion_style: Option<u32>,
    locomotion_autonomous: bool,
    locomotion_run_rate: Option<f32>,
    locomotion_controls: Option<bace_motion::LocomotionAxes>,
    authorized_jump: Option<f32>,
    capabilities: Capabilities,
    radius: f32,
    last_sequence: Option<u32>,
    intent: MotionIntent,
    jump_pending: bool,
    dynamic_contact: Option<u32>,
    turn: f32,
    maximum_turn_rate: f32,
    last_turn_sequence: Option<u32>,
    server_turn: Option<TurnControl>,
    last_server_turn: Option<TurnControl>,
    server_move: Option<(TurnControl, MotionIntent)>,
    server_move_speed: f32,
    last_server_move: Option<TurnControl>,
    last_issued_control: Option<TurnControl>,
}

impl Body {
    /// Read projection of the actual prepared locomotion interpreter. Rendering
    /// adapters must not derive an animation command from accepted velocity.
    pub fn locomotion_projection(&self) -> Option<(u32, bace_motion::MotionDrive)> {
        Some((self.locomotion_style?, self.locomotion?))
    }
    pub fn animated_locomotion(&self) -> Option<&std::sync::Arc<bace_motion::AnimatedLocomotion>> {
        self.animated.as_ref()
    }
    pub fn stopped_locomotion(
        &self,
    ) -> Option<(&std::sync::Arc<bace_motion::AnimatedLocomotion>, f32)> {
        self.stopped_locomotion.as_ref().map(|(p, r)| (p, *r))
    }
    pub fn locomotion_autonomous(&self) -> bool {
        self.locomotion_autonomous
    }
    pub fn locomotion_run_rate(&self) -> Option<f32> {
        self.locomotion_run_rate
    }
    pub fn collision_shape(&self) -> Option<&std::sync::Arc<crate::CollisionShape>> {
        self.geometry_shape.as_ref()
    }
    /// Server-derived capabilities change only after their owning character or
    /// inventory mutation commits. Accepted physical state remains unchanged.
    pub fn update_capabilities(&mut self, capabilities: Capabilities) -> Result<(), PhysicsError> {
        let capabilities = capabilities.validate()?;
        if self.server_move.is_some() && capabilities.speed * self.server_move_speed > 50.0 {
            return Err(PhysicsError::InvalidState);
        }
        self.capabilities = capabilities;
        Ok(())
    }
    pub fn spawn_geometry(
        region: &crate::GeometryRegion,
        input: GeometrySpawn,
    ) -> Result<Self, PhysicsError> {
        let GeometrySpawn {
            cell,
            position,
            shape,
            capabilities,
            heading,
            maximum_turn_rate,
        } = input;
        if !heading.is_finite()
            || !maximum_turn_rate.is_finite()
            || !(0.0..=20.0).contains(&maximum_turn_rate)
        {
            return Err(PhysicsError::InvalidState);
        }
        region.validate_placement(cell, position, &shape, &[], 0)?;
        let radius = shape.spheres().iter().map(|s| s.radius).fold(0.0, f32::max);
        Ok(Self {
            accepted: AcceptedState {
                position,
                velocity: Vec3::ZERO,
                grounded: false,
                epoch: 0,
                heading: heading.rem_euclid(std::f32::consts::TAU),
            },
            geometry_shape: Some(shape),
            animated: None,
            stopped_locomotion: None,
            root_cursor: Default::default(),
            motion_root: None,
            locomotion: None,
            locomotion_style: None,
            locomotion_autonomous: false,
            locomotion_run_rate: None,
            locomotion_controls: None,
            authorized_jump: None,
            capabilities: capabilities.validate()?,
            radius,
            last_sequence: None,
            intent: MotionIntent::new(Vec3::ZERO, false)?,
            jump_pending: false,
            dynamic_contact: None,
            turn: 0.0,
            maximum_turn_rate,
            last_turn_sequence: None,
            server_turn: None,
            last_server_turn: None,
            server_move: None,
            server_move_speed: 1.0,
            last_server_move: None,
            last_issued_control: None,
        })
    }
    pub fn step_geometry(
        &mut self,
        region: &crate::GeometryRegion,
        cell: u32,
        actor: u32,
        dynamics: &[crate::DynamicSphere],
    ) -> Result<u32, PhysicsError> {
        self.step_geometry_with_access(region, cell, actor, dynamics, &[])
    }
    pub fn step_geometry_with_access(
        &mut self,
        region: &crate::GeometryRegion,
        cell: u32,
        actor: u32,
        dynamics: &[crate::DynamicSphere],
        allowed_restrictions: &[u32],
    ) -> Result<u32, PhysicsError> {
        let shape = self
            .geometry_shape
            .as_ref()
            .ok_or(PhysicsError::InvalidState)?;
        let mut velocity = if let Some((_, intent)) = self.server_move {
            intent.velocity(self.capabilities) * self.server_move_speed
        } else if let Some(drive) = self.locomotion {
            let (sin, cos) = self.accepted.heading.sin_cos();
            let v = drive.local_velocity;
            Vec3::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos, v.z)
        } else {
            self.intent.velocity(self.capabilities)
        };
        let mut root_cursor = self.root_cursor;
        let mut root_turn = self.motion_root.map(|root| root.heading);
        if let Some(root) = self.motion_root {
            let (s, c) = self.accepted.heading.sin_cos();
            let v = root.translation * (1.0 / STEP_SECONDS);
            velocity = if self.accepted.grounded {
                Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
            } else {
                self.accepted.velocity
            };
        }
        if self.motion_root.is_none()
            && self.server_move.is_none()
            && let (Some(animated), Some(drive)) = (&self.animated, self.locomotion)
        {
            let delta = animated
                .cycle(drive.forward_motion)
                .advance(&mut root_cursor, STEP_SECONDS, drive.forward_rate, drive)
                .map_err(|_| PhysicsError::InvalidState)?;
            let (s, c) = self.accepted.heading.sin_cos();
            let v = delta.translation * (1.0 / STEP_SECONDS);
            velocity = if self.accepted.grounded {
                Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
            } else {
                self.accepted.velocity
            };
            root_turn = Some(delta.heading);
        }
        velocity.z = if self.accepted.grounded {
            0.0
        } else {
            self.accepted.velocity.z
        };
        let jumping = self.authorized_jump.filter(|_| self.accepted.grounded);
        if let Some(impulse) = jumping {
            velocity.z = impulse;
        }
        let gravity = if !self.accepted.grounded || jumping.is_some() {
            -9.8
        } else {
            0.0
        };
        // ACE PhysicsObj integrates displacement with half acceleration, then
        // updates velocity. Ground contact must not inject downhill idle drift.
        velocity.z += gravity * STEP_SECONDS * 0.5;
        let next = region.move_body(crate::GeometryStep {
            cell,
            position: self.accepted.position,
            velocity,
            seconds: STEP_SECONDS,
            shape,
            dynamics,
            ignore: actor,
            was_grounded: self.accepted.grounded && jumping.is_none(),
            allowed_restrictions,
            player_status: dynamics
                .iter()
                .find(|d| d.object == actor)
                .and_then(|d| d.player_status),
        })?;
        let mut final_velocity = next.velocity;
        if !next.grounded && final_velocity.z == velocity.z {
            final_velocity.z += gravity * STEP_SECONDS * 0.5;
        }
        self.accepted = AcceptedState {
            position: next.position,
            velocity: final_velocity,
            grounded: next.grounded,
            epoch: self.accepted.epoch,
            heading: (self.accepted.heading
                + if self.server_turn.is_some() || self.turn != 0.0 {
                    self.turn * self.maximum_turn_rate * STEP_SECONDS
                } else {
                    root_turn.unwrap_or_else(|| {
                        self.locomotion.map_or(0.0, |d| d.angular_velocity) * STEP_SECONDS
                    })
                })
            .rem_euclid(std::f32::consts::TAU),
        };
        self.root_cursor = root_cursor;
        self.motion_root = None;
        self.jump_pending = false;
        self.authorized_jump = None;
        self.dynamic_contact = next.contacted_object;
        Ok(next.cell)
    }
    pub fn teleport_geometry(
        &mut self,
        region: &crate::GeometryRegion,
        cell: u32,
        position: Vec3,
        actor: u32,
        dynamics: &[crate::DynamicSphere],
    ) -> Result<(), PhysicsError> {
        let shape = self
            .geometry_shape
            .as_ref()
            .ok_or(PhysicsError::InvalidState)?;
        region.validate_placement(cell, position, shape, dynamics, actor)?;
        self.accepted = AcceptedState {
            position,
            velocity: Vec3::ZERO,
            grounded: false,
            epoch: self.accepted.epoch.wrapping_add(1),
            heading: self.accepted.heading,
        };
        self.last_sequence = None;
        self.server_move = None;
        self.last_turn_sequence = None;
        self.stop_motion();
        self.restore_stopped_locomotion();
        Ok(())
    }
    pub fn spawn(
        scene: &SyntheticScene,
        position: Vec3,
        radius: f32,
        capabilities: Capabilities,
    ) -> Result<Self, PhysicsError> {
        Self::spawn_oriented(scene, position, radius, capabilities, 0.0, 0.0)
    }
    /// Initial heading/rate come from admitted assets/character state. The
    /// legacy synthetic constructor disables turning until explicitly prepared.
    pub fn spawn_oriented(
        scene: &SyntheticScene,
        position: Vec3,
        radius: f32,
        capabilities: Capabilities,
        heading: f32,
        maximum_turn_rate: f32,
    ) -> Result<Self, PhysicsError> {
        if !heading.is_finite()
            || !maximum_turn_rate.is_finite()
            || !(0.0..=20.0).contains(&maximum_turn_rate)
        {
            return Err(PhysicsError::InvalidState);
        }
        if !scene.valid_placement(position, radius) {
            return Err(PhysicsError::InvalidState);
        }
        let capabilities = capabilities.validate()?;
        let (position, velocity, grounded) = scene.move_body(position, Vec3::ZERO, radius, 0.0);
        Ok(Self {
            geometry_shape: None,
            animated: None,
            stopped_locomotion: None,
            root_cursor: Default::default(),
            motion_root: None,
            locomotion: None,
            locomotion_style: None,
            locomotion_autonomous: false,
            locomotion_run_rate: None,
            locomotion_controls: None,
            authorized_jump: None,
            accepted: AcceptedState {
                position,
                velocity,
                grounded,
                epoch: 0,
                heading: heading.rem_euclid(std::f32::consts::TAU),
            },
            capabilities,
            radius,
            last_sequence: None,
            intent: MotionIntent::new(Vec3::ZERO, false)?,
            jump_pending: false,
            dynamic_contact: None,
            turn: 0.0,
            maximum_turn_rate,
            last_turn_sequence: None,
            server_turn: None,
            last_server_turn: None,
            server_move: None,
            server_move_speed: 1.0,
            last_server_move: None,
            last_issued_control: None,
        })
    }
    /// Call only after the character owner preflights its stamina debit. A raw
    /// movement intent cannot authorize a geometry-body jump.
    pub fn validate_jump_command(
        &self,
        epoch: u16,
        sequence: u32,
        height: f32,
    ) -> Result<(), PhysicsError> {
        if epoch != self.accepted.epoch {
            return Err(PhysicsError::StaleEpoch);
        }
        if self.last_sequence.is_some_and(|old| {
            let advance = sequence.wrapping_sub(old);
            advance == 0 || advance >= 0x80000000
        }) {
            return Err(PhysicsError::StaleSequence);
        }
        if self.geometry_shape.is_none()
            || !self.accepted.grounded
            || self.authorized_jump.is_some()
            || !height.is_finite()
            || !(0.0..=100.0).contains(&height)
        {
            return Err(PhysicsError::InvalidState);
        }
        Ok(())
    }
    pub fn authorize_jump_command(
        &mut self,
        epoch: u16,
        sequence: u32,
        height: f32,
    ) -> Result<(), PhysicsError> {
        self.validate_jump_command(epoch, sequence, height)?;
        self.authorize_jump(height)?;
        self.last_sequence = Some(sequence);
        self.server_move = None;
        self.server_turn = None;
        Ok(())
    }
    pub fn authorize_jump(&mut self, height: f32) -> Result<(), PhysicsError> {
        if self.geometry_shape.is_none()
            || !self.accepted.grounded
            || self.authorized_jump.is_some()
            || !height.is_finite()
            || !(0.0..=100.0).contains(&height)
        {
            return Err(PhysicsError::InvalidState);
        }
        self.authorized_jump = Some((2.0 * 9.8 * height).sqrt());
        Ok(())
    }
    pub fn submit_locomotion(
        &mut self,
        epoch: u16,
        sequence: u32,
        profile: &bace_motion::LocomotionProfile,
        controls: bace_motion::LocomotionControls,
        run_rate: f32,
    ) -> Result<bace_motion::MotionDrive, PhysicsError> {
        let drive = profile.interpret(controls, run_rate)?;
        self.submit_intent(epoch, sequence, MotionIntent::new(Vec3::ZERO, false)?)?;
        self.turn = 0.0;
        self.locomotion = Some(drive);
        self.locomotion_autonomous = false;
        self.locomotion_style = Some(profile.style);
        self.locomotion_run_rate = Some(run_rate);
        self.locomotion_controls = Some(controls.into());
        Ok(drive)
    }
    pub fn submit_animated_locomotion(
        &mut self,
        epoch: u16,
        sequence: u32,
        profile: std::sync::Arc<bace_motion::AnimatedLocomotion>,
        controls: bace_motion::LocomotionControls,
        run_rate: f32,
    ) -> Result<bace_motion::MotionDrive, PhysicsError> {
        let same = self
            .animated
            .as_ref()
            .is_some_and(|old| std::sync::Arc::ptr_eq(old, &profile));
        let old_motion = self.locomotion.map(|d| d.forward_motion);
        let cursor = self.root_cursor;
        let drive =
            self.submit_locomotion(epoch, sequence, &profile.profile, controls, run_rate)?;
        if same && old_motion == Some(drive.forward_motion) {
            self.root_cursor = cursor;
        }
        self.animated = Some(profile);
        Ok(drive)
    }
    pub fn submit_animated_axes(
        &mut self,
        epoch: u16,
        sequence: u32,
        profile: std::sync::Arc<bace_motion::AnimatedLocomotion>,
        controls: bace_motion::LocomotionAxes,
        run_rate: f32,
    ) -> Result<bace_motion::MotionDrive, PhysicsError> {
        let drive = profile.profile.interpret_axes(controls, run_rate)?;
        let same = self
            .animated
            .as_ref()
            .is_some_and(|old| std::sync::Arc::ptr_eq(old, &profile))
            && self
                .locomotion
                .is_some_and(|old| old.forward_motion == drive.forward_motion);
        let cursor = self.root_cursor;
        self.submit_intent(epoch, sequence, MotionIntent::new(Vec3::ZERO, false)?)?;
        self.turn = 0.0;
        self.locomotion = Some(drive);
        self.locomotion_style = Some(profile.profile.style);
        self.locomotion_run_rate = Some(run_rate);
        self.locomotion_controls = Some(controls);
        self.animated = Some(profile);
        self.locomotion_autonomous = true;
        if same {
            self.root_cursor = cursor;
        }
        Ok(drive)
    }
    /// Source CM_Style changes forward to Ready. Side/turn survive unless the
    /// destination cycle forbids modifiers. No client epoch/sequence is consumed.
    pub fn validate_animated_style(
        &self,
        profile: &bace_motion::AnimatedLocomotion,
        clear_modifiers: bool,
    ) -> Result<bace_motion::MotionDrive, PhysicsError> {
        let mut controls = self.locomotion_controls.unwrap_or_default();
        controls.forward = 0.0;
        if clear_modifiers {
            controls.sidestep = 0.0;
            controls.turn = 0.0;
        }
        Ok(profile
            .profile
            .interpret_axes(controls, self.locomotion_run_rate.unwrap_or(1.0))?)
    }
    pub fn adopt_animated_style(
        &mut self,
        profile: std::sync::Arc<bace_motion::AnimatedLocomotion>,
        clear_modifiers: bool,
    ) -> Result<(), PhysicsError> {
        let drive = self.validate_animated_style(&profile, clear_modifiers)?;
        let mut controls = self.locomotion_controls.unwrap_or_default();
        controls.forward = 0.0;
        if clear_modifiers {
            controls.sidestep = 0.0;
            controls.turn = 0.0;
        }
        self.locomotion_controls = Some(controls);
        self.locomotion = Some(drive);
        self.locomotion_style = Some(profile.profile.style);
        self.locomotion_autonomous = false;
        self.locomotion_run_rate = Some(self.locomotion_run_rate.unwrap_or(1.0));
        self.animated = Some(profile);
        self.root_cursor = Default::default();
        Ok(())
    }
    pub fn refresh_locomotion(
        &mut self,
        profile: &bace_motion::LocomotionProfile,
        run_rate: f32,
    ) -> Result<(), PhysicsError> {
        if let Some(controls) = self.locomotion_controls {
            self.locomotion = Some(profile.interpret_axes(controls, run_rate)?);
            self.locomotion_style = Some(profile.style);
            self.locomotion_run_rate = Some(run_rate);
        }
        Ok(())
    }
    /// Server-only heading component of a validated explicit teleport.
    pub fn server_heading(&mut self, heading: f32) -> Result<(), PhysicsError> {
        if !heading.is_finite() {
            return Err(PhysicsError::InvalidState);
        }
        self.accepted.heading = heading.rem_euclid(std::f32::consts::TAU);
        Ok(())
    }
    /// Prepared server animation output; always goes through collision before
    /// becoming accepted state. Never a client pose setter.
    pub fn apply_motion_root(&mut self, root: bace_motion::RootFrame) -> Result<(), PhysicsError> {
        if !root.translation.is_finite() || !root.heading.is_finite() {
            return Err(PhysicsError::InvalidState);
        }
        self.motion_root = Some(root);
        Ok(())
    }
    pub fn clear_motion_root(&mut self) {
        self.motion_root = None;
    }
    pub fn has_locomotion_intent(&self) -> bool {
        self.intent.velocity(self.capabilities).length_squared() > 0.0
            || self
                .locomotion_controls
                .is_some_and(|c| c.forward != 0.0 || c.sidestep != 0.0 || c.turn != 0.0)
            || self.manual_turning()
    }
    pub fn accepted(&self) -> AcceptedState {
        self.accepted
    }
    pub fn collision_radius(&self) -> f32 {
        self.radius
    }
    pub fn dynamic_contact(&self) -> Option<u32> {
        self.dynamic_contact
    }
    fn restore_stopped_locomotion(&mut self) {
        if let Some((profile, run_rate)) = self.stopped_locomotion.take() {
            self.adopt_animated_style(profile.clone(), true)
                .expect("previously accepted style");
            self.refresh_locomotion(&profile.profile, run_rate)
                .expect("previously accepted run rate");
        }
    }
    /// Authoritative death/cancellation stop; accepted placement/contact still
    /// come from physics and gravity remains active.
    pub fn stop_motion(&mut self) {
        if let Some(profile) = self.animated.as_ref() {
            self.stopped_locomotion =
                Some((profile.clone(), self.locomotion_run_rate.unwrap_or(1.0)));
        }
        self.motion_root = None;
        self.server_move = None;
        self.locomotion = None;
        self.locomotion_style = None;
        self.locomotion_autonomous = false;
        self.locomotion_run_rate = None;
        self.animated = None;
        self.root_cursor = Default::default();
        self.locomotion_controls = None;
        self.authorized_jump = None;
        self.intent = MotionIntent::new(Vec3::ZERO, false).expect("finite zero intent");
        self.jump_pending = false;
        self.turn = 0.0;
        self.server_turn = None;
    }
    pub fn submit_turn_intent(
        &mut self,
        epoch: u16,
        sequence: u32,
        intent: TurnIntent,
    ) -> Result<(), PhysicsError> {
        if epoch != self.accepted.epoch {
            return Err(PhysicsError::StaleEpoch);
        }
        if self.last_turn_sequence.is_some_and(|last| {
            let distance = sequence.wrapping_sub(last);
            distance == 0 || distance >= 0x80000000
        }) {
            return Err(PhysicsError::StaleSequence);
        }
        self.last_turn_sequence = Some(sequence);
        self.server_move = None;
        self.turn = intent.axis();
        self.server_turn = None;
        Ok(())
    }
    pub fn manual_turning(&self) -> bool {
        self.server_turn.is_none()
            && (self.turn != 0.0 || self.locomotion_controls.is_some_and(|c| c.turn != 0.0))
    }
    pub fn maximum_turn_rate(&self) -> f32 {
        self.maximum_turn_rate
    }
    pub fn server_turn(&self) -> Option<TurnControl> {
        self.server_turn
    }
    pub fn begin_server_turn(
        &mut self,
        epoch: u16,
        control: TurnControl,
        intent: TurnIntent,
    ) -> Result<(), PhysicsError> {
        if epoch != self.accepted.epoch {
            return Err(PhysicsError::StaleEpoch);
        }
        if control.owner == 0
            || control.sequence == 0
            || self.last_server_turn.is_some_and(|old| control <= old)
        {
            return Err(PhysicsError::StaleSequence);
        }
        self.server_turn = Some(control);
        self.last_server_turn = Some(control);
        self.turn = intent.axis();
        Ok(())
    }
    pub fn continue_server_turn(
        &mut self,
        epoch: u16,
        control: TurnControl,
        intent: TurnIntent,
    ) -> Result<(), PhysicsError> {
        if epoch != self.accepted.epoch {
            return Err(PhysicsError::StaleEpoch);
        }
        if self.server_turn != Some(control) {
            return Err(PhysicsError::StaleSequence);
        }
        self.turn = intent.axis();
        Ok(())
    }
    pub fn finish_server_turn(&mut self, control: TurnControl) {
        if self.server_turn == Some(control) {
            self.server_turn = None;
            self.turn = 0.0;
        }
    }
    /// Revalidate placement when transferring ownership to another scene.
    /// Constructing a body in one scene cannot authorize entry into another.
    pub fn validate_placement(&self, scene: &SyntheticScene) -> Result<(), PhysicsError> {
        if self.geometry_shape.is_none()
            && scene.valid_placement(self.accepted.position, self.radius)
        {
            Ok(())
        } else {
            Err(PhysicsError::InvalidState)
        }
    }
    pub fn submit_intent(
        &mut self,
        epoch: u16,
        sequence: u32,
        intent: MotionIntent,
    ) -> Result<(), PhysicsError> {
        if self.geometry_shape.is_some() && intent.wants_jump() {
            return Err(PhysicsError::InvalidState);
        }
        if epoch != self.accepted.epoch {
            return Err(PhysicsError::StaleEpoch);
        }
        if let Some(last) = self.last_sequence {
            let advance = sequence.wrapping_sub(last);
            if advance == 0 || advance >= 0x8000_0000 {
                return Err(PhysicsError::StaleSequence);
            }
        }
        self.last_sequence = Some(sequence);
        self.server_move = None;
        if self.server_turn.take().is_some() {
            self.turn = 0.0;
        }
        self.jump_pending = intent.wants_jump();
        self.intent = intent;
        self.locomotion = None;
        self.locomotion_style = None;
        self.locomotion_autonomous = false;
        self.locomotion_run_rate = None;
        self.animated = None;
        self.root_cursor = Default::default();
        self.locomotion_controls = None;
        Ok(())
    }
    pub fn step(&mut self, scene: &SyntheticScene) {
        // An authentic body can never be advanced by the legacy test solver.
        if self.geometry_shape.is_some() {
            return;
        }
        let mut velocity = self
            .server_move
            .map_or(self.intent, |(_, intent)| intent)
            .velocity(self.capabilities)
            * if self.server_move.is_some() {
                self.server_move_speed
            } else {
                1.0
            };
        let root = self.motion_root.take();
        if let Some(root) = root {
            let (s, c) = self.accepted.heading.sin_cos();
            let v = root.translation * (1.0 / STEP_SECONDS);
            velocity = Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z);
        }
        velocity.z = self.accepted.velocity.z;
        if self.jump_pending && self.accepted.grounded {
            velocity.z = self.capabilities.jump_impulse;
        }
        self.jump_pending = false;
        velocity.z -= 9.8 * STEP_SECONDS;
        self.dynamic_contact =
            scene.dynamic_contact(self.accepted.position, velocity * STEP_SECONDS, self.radius);
        let (position, velocity, grounded) =
            scene.move_body(self.accepted.position, velocity, self.radius, STEP_SECONDS);
        self.accepted = AcceptedState {
            position,
            velocity,
            grounded,
            epoch: self.accepted.epoch,
            heading: (self.accepted.heading
                + root.map_or(self.turn * self.maximum_turn_rate * STEP_SECONDS, |r| {
                    r.heading
                }))
            .rem_euclid(std::f32::consts::TAU),
        };
    }
    /// Server-only privileged relocation, never a decoded client position path.
    pub fn server_teleport(
        &mut self,
        scene: &SyntheticScene,
        position: Vec3,
    ) -> Result<(), PhysicsError> {
        if self.geometry_shape.is_some() || !scene.valid_placement(position, self.radius) {
            return Err(PhysicsError::InvalidState);
        }
        let (position, velocity, grounded) =
            scene.move_body(position, Vec3::ZERO, self.radius, 0.0);
        self.accepted = AcceptedState {
            position,
            velocity,
            grounded,
            epoch: self.accepted.epoch.wrapping_add(1),
            heading: self.accepted.heading,
        };
        self.last_sequence = None;
        self.server_move = None;
        self.last_turn_sequence = None;
        self.turn = 0.0;
        self.server_turn = None;
        self.intent = MotionIntent::new(Vec3::ZERO, false)?;
        self.jump_pending = false;
        self.stop_motion();
        self.restore_stopped_locomotion();
        Ok(())
    }
    /// Observation cannot mutate authority. Full history-based reconciliation is
    /// deferred until AC motion/DAT paths are implemented and client-tested.
    pub fn observe(&self, epoch: u16, position: Vec3) -> Result<AcceptedState, PhysicsError> {
        if epoch != self.accepted.epoch {
            return Err(PhysicsError::StaleEpoch);
        }
        if !position.is_finite() {
            return Err(PhysicsError::InvalidState);
        }
        Ok(self.accepted)
    }
}

pub struct GeometrySpawn {
    pub cell: u32,
    pub position: Vec3,
    pub shape: std::sync::Arc<crate::CollisionShape>,
    pub capabilities: Capabilities,
    pub heading: f32,
    pub maximum_turn_rate: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum PhysicsError {
    #[error("invalid physical state or unavailable placement")]
    InvalidState,
    #[error("stale movement epoch")]
    StaleEpoch,
    #[error("duplicate or stale movement sequence")]
    StaleSequence,
    #[error(transparent)]
    Geometry(#[from] crate::GeometryError),
    #[error(transparent)]
    Motion(#[from] MotionError),
}
