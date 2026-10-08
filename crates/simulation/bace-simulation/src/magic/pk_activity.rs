//! GDLE per-effect PK timer gates. Projectile contact tags either player mode;
//! direct harm/transfer requires PK bit4. Enchantment retains the source
//! top-level-player owner condition after its resist branch.
use super::*;
fn is_pk(world: &World, policy: &Combat, actor: EntityId) -> bool {
    let raw = world
        .combatant(actor)
        .and_then(|c| c.death_pk_status())
        .or_else(|| {
            match world
                .properties(actor)
                .and_then(|p| p.get(bace_entity::PropertyFamily::Int, 134))
            {
                Some(bace_entity::PropertyValue::Int(value)) => Some(*value as u32),
                _ => None,
            }
        });
    raw.map_or_else(
        || {
            policy
                .physical_profile(actor)
                .is_some_and(|p| p.pk == bace_gameplay_api::weapon_combat::PkStatus::Pk)
        },
        |v| v & 4 != 0,
    )
}
pub(super) fn direct(
    world: &mut World,
    policy: &Combat,
    source: EntityId,
    target: EntityId,
    now: f64,
) {
    if is_pk(world, policy, source) && is_pk(world, policy, target) {
        crate::pk_activity::record_pair(world, source, target, now);
    }
}
pub(super) fn enchantment(
    world: &mut World,
    policy: &Combat,
    source: EntityId,
    owner: EntityId,
    now: f64,
) {
    if is_pk(world, policy, source) {
        crate::pk_activity::record_pair(world, source, owner, now);
    }
}
