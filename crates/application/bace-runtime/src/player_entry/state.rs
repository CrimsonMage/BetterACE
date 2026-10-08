//! ACE WorldObject_Networking.CalculatedPhysicsState and Player.InitPhysicsObj.
//! This adapter only projects the accepted, idle login owner; it never creates a
//! replacement movement owner or reads a saved/client-requested position.
use super::{EntryObjectState, instance_from_login, object::Properties};
use bace_content::WeenieV1;
use bace_dat::ModelSetup;
use bace_persistence::OnlineLoginReceipt;
use bace_replication::{SequenceKind as K, Sequences};
use bace_simulation::PlayerReadSnapshot;
use bace_storage_codec::PlayerSaveV6;
use bace_wire::{
    InterpretedMotion, MotionBody, MovementDescription, PhysicsMovement, PhysicsSequences,
    WirePosition,
};

/// Reads the existing counter owner without consuming or resetting any sequence.
pub fn physics_sequences(owner: &Sequences) -> PhysicsSequences {
    PhysicsSequences {
        position: owner.current(K::ObjectPosition, 0),
        movement: owner.current(K::ObjectMovement, 0),
        state: owner.current(K::ObjectState, 0),
        vector: owner.current(K::ObjectVector, 0),
        teleport: owner.current(K::ObjectTeleport, 0),
        server_control: owner.current(K::ObjectServerControl, 0),
        force_position: owner.current(K::ObjectForcePosition, 0),
        visual_description: owner.current(K::ObjectVisualDesc, 0),
        instance: owner.current(K::ObjectInstance, 0),
    }
}

pub fn prepare_player_entry_state(
    saved: &PlayerSaveV6,
    snapshot: &PlayerReadSnapshot,
    receipt: OnlineLoginReceipt,
    sequences: &Sequences,
    setup: &ModelSetup,
) -> Result<EntryObjectState, String> {
    let binding = snapshot.binding();
    let physics = snapshot.entry_physics();
    let world = snapshot.world();
    let motion = snapshot
        .entry_motion()
        .ok_or("entry motion owner missing")?;
    let counters = physics_sequences(sequences);
    if saved.player.entity.object_id != binding.actor.0
        || counters.instance != instance_from_login(receipt, binding)?
        || counters.teleport != physics.epoch()
        || snapshot.operation().is_some()
        || world.position != physics.position()
        || world.heading != physics.heading_radians()
        || world.cell.0 == 0
    {
        return Err("entry snapshot/counter identity mismatch".into());
    }
    // Full moving/turning state needs its own accepted projection. Login is held
    // before client action admission; never substitute neutral motion for it.
    if motion.style != 0x8000003d
        || motion.substate != 0x41000003
        || motion.speed != 1.0
        || physics.velocity() != bace_geometry::Vec3::ZERO
        || !physics.grounded()
    {
        return Err("entry requires accepted grounded NonCombat/Ready state".into());
    }
    let source = &saved.player.entity.state;
    let p = Properties(source);
    if p.did("Setup") != Some(setup.id) {
        return Err("entry setup identity mismatch".into());
    }
    let cloak =
        u32::try_from(p.int("CloakStatus").unwrap_or(0)).map_err(|_| "entry cloak status")?;
    if cloak > 4 {
        return Err("entry cloak status".into());
    }
    let position = physics.position();
    let half_heading = physics.heading_radians() * 0.5;
    if !position.is_finite() || !half_heading.is_finite() {
        return Err("entry nonfinite accepted transform".into());
    }
    Ok(EntryObjectState {
        is_player: true,
        is_creature: true,
        physics_state: initial_physics_state(source, setup.flags, true),
        position: Some(WirePosition {
            cell: world.cell.0,
            origin: [position.x, position.y, position.z],
            rotation: [half_heading.cos(), 0.0, 0.0, half_heading.sin()],
        }),
        movement: Some(PhysicsMovement::Motion(MovementDescription {
            autonomous: false,
            motion_flags: 0,
            current_style: motion.style as u16,
            body: MotionBody::State {
                state: InterpretedMotion {
                    current_style: Some(motion.style as u16),
                    forward_command: Some(motion.substate as u16),
                    ..InterpretedMotion::default()
                },
                sticky_object: None,
            },
        })),
        parent: None,
        children: vec![],
        velocity: [0.0; 3],
        acceleration: [0.0; 3],
        omega: [0.0; 3],
        sequences: counters,
        admin_vision: false,
        change_no_draw: false,
        cloak_status: cloak,
    })
}

/// Before a PhysicsObj exists, ACE ignores saved Int93 for players and uses its
/// pink-bubble defaults. Nullable bools override default bits; BSP comes from DAT.
pub fn initial_physics_state(source: &WeenieV1, setup_flags: u32, player: bool) -> u32 {
    let p = Properties(source);
    let mut state = if player {
        0x00404410
    } else {
        p.int("PhysicsState").map_or(0x00400c08, |v| v as u32)
    };
    // These nullable wrappers read GetPhysicsState(), which returns false while
    // PhysicsObj is absent (even when the default Int93 contains the bit).
    state &= !(1 | 64 | 128 | 256 | 512 | 4096 | 16384 | 1048576 | 8388608);
    for (name, bit) in [
        ("Ethereal", 4),
        ("ReportCollisions", 8),
        ("IgnoreCollisions", 16),
        ("NoDraw", 32),
        ("GravityStatus", 1024),
        ("LightsStatus", 2048),
        ("ScriptedCollision", 32768),
        ("Inelastic", 131072),
        ("ReportCollisionsAsEnvironment", 2097152),
        ("AllowEdgeSlide", 4194304),
        ("IsFrozen", 16777216),
    ] {
        if let Some(enabled) = p.bool(name) {
            if enabled {
                state |= bit;
            } else {
                state &= !bit;
            }
        }
    }
    state &= !(0x10000 | 0x40000 | 0x80000);
    if setup_flags & 8 != 0 {
        state |= 0x10000;
    }
    // Player.InitPhysicsObj runs after the base constructor's flag calculation.
    if player {
        state = (state | 0x4010) & !8;
    }
    state
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
