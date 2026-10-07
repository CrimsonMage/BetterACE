//! WorldObject.SerializePhysicsData layout. State, authority and optional-field
//! selection are explicit projection inputs; no default physics is fabricated.
use crate::position::write_vector;
use crate::{MovementDescription, WireError, WirePosition, Writer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicsParent {
    pub object_id: u32,
    pub location: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicsChild {
    pub object_id: u32,
    pub location: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicsSequences {
    pub position: u16,
    pub movement: u16,
    pub state: u16,
    pub vector: u16,
    pub teleport: u16,
    pub server_control: u16,
    pub force_position: u16,
    pub visual_description: u16,
    pub instance: u16,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PhysicsMovement {
    Motion(MovementDescription),
    AnimationFrame(u32),
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhysicsOptions {
    pub movement: Option<PhysicsMovement>,
    pub position: Option<WirePosition>,
    pub motion_table: Option<u32>,
    pub sound_table: Option<u32>,
    pub physics_table: Option<u32>,
    pub setup: Option<u32>,
    pub parent: Option<PhysicsParent>,
    pub children: Vec<PhysicsChild>,
    pub scale: Option<f32>,
    pub friction: Option<f32>,
    pub elasticity: Option<f32>,
    pub translucency: Option<f32>,
    pub velocity: Option<[f32; 3]>,
    pub acceleration: Option<[f32; 3]>,
    pub omega: Option<[f32; 3]>,
    pub default_script: Option<u32>,
    pub default_script_intensity: Option<f32>,
}
impl PhysicsOptions {
    pub fn flags(&self) -> u32 {
        let mut flags = 0;
        for (present, flag) in [
            (self.setup.is_some(), 0x1),
            (self.motion_table.is_some(), 0x2),
            (self.velocity.is_some(), 0x4),
            (self.acceleration.is_some(), 0x8),
            (self.omega.is_some(), 0x10),
            (self.parent.is_some(), 0x20),
            (!self.children.is_empty(), 0x40),
            (self.scale.is_some(), 0x80),
            (self.friction.is_some(), 0x100),
            (self.elasticity.is_some(), 0x200),
            (self.sound_table.is_some(), 0x800),
            (self.physics_table.is_some(), 0x1000),
            (self.default_script.is_some(), 0x2000),
            (self.default_script_intensity.is_some(), 0x4000),
            (self.position.is_some(), 0x8000),
            (
                matches!(self.movement, Some(PhysicsMovement::Motion(_))),
                0x10000,
            ),
            (
                matches!(self.movement, Some(PhysicsMovement::AnimationFrame(_))),
                0x20000,
            ),
            (self.translucency.is_some(), 0x40000),
        ] {
            if present {
                flags |= flag;
            }
        }
        flags
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsDescription {
    pub state: u32,
    pub options: PhysicsOptions,
    pub sequences: PhysicsSequences,
}
impl PhysicsDescription {
    pub fn encode(
        &self,
        max_children: usize,
        max_motion_commands: usize,
    ) -> Result<Vec<u8>, WireError> {
        let mut writer = Writer::new();
        self.write(&mut writer, max_children, max_motion_commands)?;
        Ok(writer.into_bytes())
    }
    pub(crate) fn write(
        &self,
        writer: &mut Writer,
        max_children: usize,
        max_motion_commands: usize,
    ) -> Result<(), WireError> {
        let value = &self.options;
        if value.children.len() > max_children || value.children.len() > i32::MAX as usize {
            return Err(WireError::LimitExceeded);
        }
        writer.u32(value.flags());
        writer.u32(self.state);
        if let Some(movement) = &value.movement {
            match movement {
                PhysicsMovement::Motion(motion) => {
                    let data = motion.encode(max_motion_commands)?;
                    writer.u32(u32::try_from(data.len()).map_err(|_| WireError::LimitExceeded)?);
                    writer.bytes(&data);
                    writer.u32(u32::from(motion.autonomous));
                }
                PhysicsMovement::AnimationFrame(frame) => writer.u32(*frame),
            }
        }
        if let Some(position) = value.position {
            position.write(writer);
        }
        for id in [
            value.motion_table,
            value.sound_table,
            value.physics_table,
            value.setup,
        ]
        .into_iter()
        .flatten()
        {
            writer.u32(id);
        }
        if let Some(parent) = value.parent {
            writer.u32(parent.object_id);
            writer.u32(parent.location);
        }
        if !value.children.is_empty() {
            writer.u32(value.children.len() as u32);
            for child in &value.children {
                writer.u32(child.object_id);
                writer.u32(child.location);
            }
        }
        for v in [
            value.scale,
            value.friction,
            value.elasticity,
            value.translucency,
        ]
        .into_iter()
        .flatten()
        {
            writer.f32(v);
        }
        for v in [value.velocity, value.acceleration, value.omega]
            .into_iter()
            .flatten()
        {
            write_vector(writer, v);
        }
        if let Some(v) = value.default_script {
            writer.u32(v);
        }
        if let Some(v) = value.default_script_intensity {
            writer.f32(v);
        }
        let seq = self.sequences;
        for sequence in [
            seq.position,
            seq.movement,
            seq.state,
            seq.vector,
            seq.teleport,
            seq.server_control,
            seq.force_position,
            seq.visual_description,
            seq.instance,
        ] {
            writer.u16(sequence);
        }
        writer.align4();
        Ok(())
    }
}
