//! Pinned ACE WorldObject_Networking create projection. Runtime receives accepted
//! placement, motion and sequence owners; saved client positions are never read.
use bace_content::WeenieV1;
use bace_wire::{
    ObjectDescription, ObjectGameData, ObjectGameOptions, ObjectModel, PhysicsChild,
    PhysicsDescription, PhysicsMovement, PhysicsOptions, PhysicsParent, PhysicsSequences,
    WirePosition,
};

/// Explicit live state, already accepted by the simulation/replication owners.
#[derive(Clone, Debug)]
pub struct EntryObjectState {
    pub is_player: bool,
    pub is_creature: bool,
    pub physics_state: u32,
    pub position: Option<WirePosition>,
    pub movement: Option<PhysicsMovement>,
    pub parent: Option<PhysicsParent>,
    pub children: Vec<PhysicsChild>,
    pub velocity: [f32; 3],
    pub acceleration: [f32; 3],
    pub omega: [f32; 3],
    pub sequences: PhysicsSequences,
    pub admin_vision: bool,
    pub change_no_draw: bool,
    pub cloak_status: u32,
}
#[derive(Clone, Debug)]
pub struct PreparedEntryModel {
    pub model: ObjectModel,
    pub icon_override: Option<u32>,
}

pub fn prepare_entry_object(
    id: u32,
    source: &WeenieV1,
    appearance: PreparedEntryModel,
    live: EntryObjectState,
) -> Result<ObjectDescription, String> {
    if id == 0 || live.children.len() > 128 || live.cloak_status > 4 {
        return Err("invalid entry object identity/state".into());
    }
    if live
        .velocity
        .into_iter()
        .chain(live.acceleration)
        .chain(live.omega)
        .any(|v| !v.is_finite())
    {
        return Err("nonfinite accepted object vector".into());
    }
    let p = Properties(source);
    let mut options = PhysicsOptions {
        movement: live.movement.or_else(|| {
            p.int("Placement")
                .map(|v| PhysicsMovement::AnimationFrame(v as u32))
        }),
        position: live.position,
        motion_table: p.did("MotionTable").filter(|v| *v != 0),
        sound_table: p.did("SoundTable").filter(|v| *v != 0),
        physics_table: p.did("PhysicsEffectTable").filter(|v| *v != 0),
        setup: p.did("Setup").filter(|v| *v != 0),
        parent: live.parent,
        children: live.children,
        scale: p
            .float("DefaultScale")
            .map(|v| v as f32)
            .filter(|v| v.abs() >= 0.001),
        friction: p.float("Friction").map(|v| v as f32),
        elasticity: p.float("Elasticity").map(|v| v as f32),
        translucency: p
            .float("Translucency")
            .map(|v| v as f32)
            .filter(|v| v.abs() >= 0.001),
        velocity: (live.velocity != [0.; 3]).then_some(live.velocity),
        acceleration: (live.acceleration != [0.; 3]).then_some(live.acceleration),
        omega: (live.omega != [0.; 3]).then_some(live.omega),
        default_script: p.did("PhysicsScript"),
        default_script_intensity: p.float("PhysicsScriptIntensity").map(|v| v as f32),
    };
    if source
        .properties
        .floats
        .iter()
        .any(|p| !p.value.is_finite() || !(p.value as f32).is_finite())
    {
        return Err("nonfinite object property".into());
    }
    if live.admin_vision && live.is_player && live.cloak_status == 2 {
        options.translucency = Some(0.5);
    }
    let state = if live.change_no_draw {
        live.physics_state & !0x100020
    } else {
        live.physics_state
    };
    let mut game = game_data(source, live.is_player, live.is_creature, live.cloak_status)?;
    if let Some(icon) = appearance.icon_override {
        game.icon_id = icon;
    }
    if live.admin_vision {
        game.description_flags &= !0x80;
    }
    Ok(ObjectDescription {
        object_id: id,
        model: appearance.model,
        physics: PhysicsDescription {
            state,
            options,
            sequences: live.sequences,
        },
        game,
    })
}

pub(crate) fn prepare_vendor_game_data(source: &WeenieV1) -> Result<ObjectGameData, String> {
    game_data(source, false, false, 0)
}

fn game_data(
    source: &WeenieV1,
    player: bool,
    creature: bool,
    cloak: u32,
) -> Result<ObjectGameData, String> {
    let p = Properties(source);
    if matches!(source.weenie_type, 53 | 56) {
        return Err("house/hook descriptions require their live owner projection".into());
    }
    let wield = p.iid("Wielder");
    let workmanship = p
        .int("ItemWorkmanship")
        .map(|raw| {
            let mut value = raw as f32 / p.int("NumItemsInMaterial").unwrap_or(1) as f32;
            if !(1.0..=10.0).contains(&value) {
                value = (raw as f32 / 10000.0 / p.int("Structure").unwrap_or(1) as f32)
                    .clamp(1.0, 10.0);
            }
            value
        })
        .filter(|v| *v as u32 != 0);
    if workmanship.is_some_and(|v| !v.is_finite()) {
        return Err("invalid object workmanship".into());
    }
    let options = ObjectGameOptions {
        plural_name: p.string("PluralName").map(str::to_owned),
        item_capacity: p
            .int("ItemsCapacity")
            .filter(|_| source.weenie_type != 55)
            .map(|v| v as u8),
        container_capacity: p
            .int("ContainersCapacity")
            .filter(|_| source.weenie_type != 55)
            .map(|v| v as u8),
        ammo_type: p.int("AmmoType").map(|v| v as u16),
        value: p.int("Value").filter(|v| *v > 0),
        usable: p.int("ItemUseable").map(|v| v as u32),
        use_radius: p.float("UseRadius").map(|v| v as f32),
        target_type: p.int("TargetType").map(|v| v as u32),
        ui_effects: p.int("UiEffects").map(|v| v as u32),
        combat_use: p.int("CombatUse").map(|v| v as i8),
        structure: p.int("Structure").map(|v| v as u16),
        max_structure: p.int("MaxStructure").map(|v| v as u16),
        stack_size: p.int("StackSize").map(|v| v as u16),
        max_stack_size: p.int("MaxStackSize").map(|v| v as u16),
        container: p.iid("Container"),
        wielder: wield,
        valid_locations: p.int("ValidLocations").map(|v| v as u32),
        wielded_location: p
            .int("CurrentWieldedLocation")
            .filter(|v| *v != 0 && wield.is_some_and(|v| v != 0))
            .map(|v| v as u32),
        clothing_priority: p.int("ClothingPriority").map(|v| v as u32),
        radar_color: p.int("RadarBlipColor").map(|v| v as u8),
        radar_behavior: p.int("ShowableOnRadar").map(|v| v as u8),
        script: p.did("PhysicsScript").filter(|v| *v != 0).map(|v| v as u16),
        workmanship,
        burden: p
            .int("EncumbranceVal")
            .filter(|v| *v != 0 && !creature && source.weenie_type != 33)
            .map(|v| v as u16),
        spell: p.did("Spell").filter(|v| *v != 0).map(|v| v as u16),
        house_owner: p.iid("HouseOwner"),
        house_restrictions: None,
        hook_item_types: p.int("HookItemType").map(|v| v as u32),
        monarch: p.iid("Monarch"),
        hook_type: p.int("HookType").map(|v| v as u16),
        icon_overlay: p.did("IconOverlay").filter(|v| *v != 0),
        icon_underlay: p.did("IconUnderlay").filter(|v| *v != 0),
        material_type: p.int("MaterialType").map(|v| v as u32),
        cooldown: p.int("SharedCooldown").filter(|v| *v != 0),
        cooldown_duration: p
            .float("CooldownDuration")
            .filter(|v| (*v as f32).abs() >= 0.001),
        pet_owner: p.iid("PetOwner").filter(|v| *v != 0),
    };
    let mut flags = match source.weenie_type {
        7 => 0x40000,
        8 => 0x100,
        12 => 0x200,
        14 => 0x2000,
        18 => 0x8000,
        19 => 0x1000,
        23 => 0x20000,
        25 => 0x4000,
        28 => 0x10000,
        65 => 0x8000000,
        _ => 0,
    };
    if player {
        flags |= 8;
    }
    if matches!(source.weenie_type, 11 | 41) && cloak < 3 {
        flags |= 0x100000;
    }
    if matches!(source.weenie_type, 11 | 41) {
        set(&mut flags, 8, cloak < 4);
    }
    if matches!(source.weenie_type, 14 | 20 | 21 | 56 | 57) {
        set(&mut flags, 1, !p.bool("Locked").unwrap_or(false));
    }
    for (name, bit, default) in [
        ("Inscribable", 2, false),
        ("Stuck", 4, false),
        ("Attackable", 0x10, true),
        ("HiddenAdmin", 0x40, false),
        ("UiHidden", 0x80, false),
        ("IgnoreHouseBarriers", 0x400000, false),
        ("RequiresBackpackSlot", 0x800000, false),
        ("Retained", 0x1000000, false),
        ("WieldOnUse", 0x20000000, false),
        ("AutowieldLeft", 0x40000000, false),
    ] {
        set(&mut flags, bit, p.bool(name).unwrap_or(default));
    }
    for (value, bit) in [(4, 0x20), (32, 0x200000), (64, 0x2000000)] {
        set(&mut flags, bit, p.int("PlayerKillerStatus") == Some(value));
    }
    Ok(ObjectGameData {
        name: p.string("Name").unwrap_or("").into(),
        class_id: source.weenie_id,
        icon_id: p.did("Icon").unwrap_or(0),
        item_type: p.int("ItemType").unwrap_or(0) as u32,
        description_flags: flags,
        options,
    })
}
fn set(flags: &mut u32, bit: u32, value: bool) {
    if value { *flags |= bit } else { *flags &= !bit }
}
/// Names resolve through the independently extracted pinned enum catalog.
pub(super) struct Properties<'a>(pub &'a WeenieV1);
impl<'a> Properties<'a> {
    fn key(kind: &str, name: &str) -> Option<u32> {
        bace_loot::ace_tables::enum_value(kind, name).and_then(|v| v.try_into().ok())
    }
    pub(super) fn int(&self, name: &str) -> Option<i32> {
        let id = Self::key("PropertyInt", name)?;
        self.0
            .properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value)
    }
    pub(super) fn did(&self, name: &str) -> Option<u32> {
        let id = Self::key("PropertyDataId", name)?;
        self.0
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value)
    }
    pub(super) fn iid(&self, name: &str) -> Option<u32> {
        let id = Self::key("PropertyInstanceId", name)?;
        self.0
            .properties
            .instance_ids
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value)
    }
    pub(super) fn float(&self, name: &str) -> Option<f64> {
        let id = Self::key("PropertyFloat", name)?;
        self.0
            .properties
            .floats
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value)
    }
    pub(super) fn bool(&self, name: &str) -> Option<bool> {
        let id = Self::key("PropertyBool", name)?;
        self.0
            .properties
            .bools
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value)
    }
    pub(super) fn string(&self, name: &str) -> Option<&'a str> {
        let id = Self::key("PropertyString", name)?;
        self.0
            .properties
            .strings
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value.as_str())
    }
}
