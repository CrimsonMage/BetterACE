//! GDLE 353cbab Player::UpdatePKActivity stores int(cur_time + 20).
//! Call sites decide exactly which accepted interaction qualifies; no damage
//! amount or a client-supplied PK timestamp is used to infer participation.
use bace_types::EntityId;
use bace_world::World;
#[cfg(test)]
mod tests;
pub(crate) fn record_pair(world: &mut World, source: EntityId, target: EntityId, now: f64) {
    if !now.is_finite()
        || now < 0.
        || ![source, target]
            .into_iter()
            .all(|id| world.combatant(id).is_some_and(|c| c.profile().player))
    {
        return;
    }
    let deadline = (now + 20.).floor();
    for actor in [source, target] {
        world
            .combatant_mut(actor)
            .expect("same-owner player preflight")
            .set_pk_activity_deadline(deadline)
            .expect("finite owner clock");
    }
}
