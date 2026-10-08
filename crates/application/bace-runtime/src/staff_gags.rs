//! Cold source properties and exact gag snapshots; SQL remains in its adapter.
use bace_content::{Property, WeenieV1};
use bace_simulation::GagRecovery;
use bace_social::GagState;
use bace_storage_codec::{PlayerSaveV6, SaveCodecError};
pub fn restore_gag(source: &WeenieV1) -> Result<GagRecovery, SaveCodecError> {
    let float = |id| {
        source
            .properties
            .floats
            .iter()
            .find(|p| p.id == id)
            .map_or(0., |p| p.value)
    };
    let state = GagState {
        active: source
            .properties
            .bools
            .iter()
            .find(|p| p.id == 111)
            .is_some_and(|p| p.value),
        timestamp: float(112),
        remaining: float(161),
        noticed: false,
    };
    state
        .validate()
        .map_err(|_| SaveCodecError::Invalid("gag properties"))?;
    Ok(GagRecovery {
        state,
        pending_interval: None,
    })
}
/// Preserve an unchanged raw optional default; semantic mutations use the source
/// removing setters. Explicit command ungag additionally canonicalizes all fields.
pub fn overlay_gag(
    source: &mut WeenieV1,
    state: GagState,
    command: bool,
) -> Result<(), SaveCodecError> {
    state
        .validate()
        .map_err(|_| SaveCodecError::Invalid("gag properties"))?;
    if command
        || source
            .properties
            .bools
            .iter()
            .find(|p| p.id == 111)
            .is_some_and(|p| p.value)
            != state.active
    {
        set(
            &mut source.properties.bools,
            111,
            state.active.then_some(true),
        );
    }
    for (id, value) in [(112, state.timestamp), (161, state.remaining)] {
        if command
            || source
                .properties
                .floats
                .iter()
                .find(|p| p.id == id)
                .map_or(0., |p| p.value)
                != value
        {
            set(
                &mut source.properties.floats,
                id,
                (value != 0.).then_some(value),
            );
        }
    }
    Ok(())
}
pub fn freeze_offline_gag(
    saved: &PlayerSaveV6,
    enabled: bool,
    unix_seconds: f64,
) -> Result<PlayerSaveV6, SaveCodecError> {
    let before = restore_gag(&saved.player.entity.state)?;
    let change = before
        .state
        .change(enabled, unix_seconds)
        .map_err(|_| SaveCodecError::Invalid("gag command"))?;
    let mut next = saved.clone();
    next.player.entity.mutation_revision = next
        .player
        .entity
        .mutation_revision
        .checked_add(1)
        .ok_or(SaveCodecError::Invalid("gag revision overflow"))?;
    overlay_gag(&mut next.player.entity.state, change.after, true)?;
    next.validate()?;
    Ok(next)
}
fn set<T>(properties: &mut Vec<Property<T>>, id: u32, value: Option<T>) {
    properties.retain(|p| p.id != id);
    if let Some(value) = value {
        properties.push(Property { id, value });
        properties.sort_by_key(|p| p.id);
    }
}
