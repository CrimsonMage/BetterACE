use super::*;

pub(super) fn vital_after_cost(
    world: &World,
    actor: EntityId,
    vital: EntityVital,
    caster: EntityId,
    cost: u32,
) -> Result<bace_entity::VitalPool, CastRejection> {
    let mut value = world
        .vital(actor, vital)
        .map_err(|_| CastRejection::MissingAssets)?;
    if actor == caster && vital == EntityVital::Mana {
        value.current = value
            .current
            .checked_sub(cost)
            .ok_or(CastRejection::InsufficientMana)?;
    }
    Ok(value)
}
pub(super) fn entity_vital(vital: Vital) -> EntityVital {
    match vital {
        Vital::Health => EntityVital::Health,
        Vital::Stamina => EntityVital::Stamina,
        Vital::Mana => EntityVital::Mana,
    }
}
pub(super) fn attempt_target(attempt: &Attempt) -> Option<EntityId> {
    attempt.target
}
pub(super) fn unit(random: &mut RandomStream) -> Result<f32, CastRejection> {
    Ok((random
        .below(1 << 24)
        .map_err(|_| CastRejection::InvalidState)? as f32)
        / (1_u32 << 24) as f32)
}
pub(super) fn roll_range(random: &mut RandomStream, a: i32, b: i32) -> Result<i32, CastRejection> {
    let lo = a.min(b);
    let hi = a.max(b);
    let width = (i64::from(hi) - i64::from(lo) + 1) as u64;
    let offset = random
        .below(width)
        .map_err(|_| CastRejection::InvalidState)? as i64;
    i32::try_from(i64::from(lo) + offset).map_err(|_| CastRejection::InvalidState)
}
pub(super) fn observe(
    world: &World,
    actor: EntityId,
    target: Option<EntityId>,
    spell: &PreparedSpell,
    skill: u32,
) -> Result<CastObservation, CastRejection> {
    observe_kind(world, actor, target, spell, skill, false)
}
pub(super) fn observe_kind(
    world: &World,
    actor: EntityId,
    target: Option<EntityId>,
    spell: &PreparedSpell,
    skill: u32,
    object: bool,
) -> Result<CastObservation, CastRejection> {
    let (cell, state) = world
        .actor_state(actor)
        .map_err(|_| CastRejection::MissingActor)?;
    let alive = match world.combatant(actor) {
        Some(combat) => combat.health() != 0,
        None if object => true,
        None => return Err(CastRejection::MissingAssets),
    };
    let projectile = matches!(
        spell.effect,
        SpellEffect::Projectile(_) | SpellEffect::LifeProjectile { .. }
    );
    let mut angle = None;
    let mut in_range = true;
    if let Some(target) = target
        && target != actor
    {
        let source = super::projectile_launch::actor_frame(world, actor, cell)?;
        let target = super::projectile_launch::actor_frame(world, target, cell)?;
        let delta = target.position - source.position;
        let range = (spell.range_constant + spell.range_per_skill * skill as f32).min(75.);
        in_range = (delta.length_squared().sqrt() - source.radius - target.radius).max(0.) <= range;
        angle = Some(
            heading_delta(state.heading_radians(), (-delta.x).atan2(delta.y))
                .map_err(|_| CastRejection::InvalidState)?
                .abs()
                .to_degrees(),
        );
    }
    Ok(CastObservation {
        cell: cell.0,
        position: state.position(),
        alive,
        in_portal: world.is_in_portal_transit(actor),
        peace_mode: accepted_peace_style(world, actor),
        heading_to_target: angle,
        target_valid: true,
        target_in_range: in_range,
        geometry_clear: if let Some(target) =
            target.filter(|target| *target != actor && !projectile)
        {
            let source = super::projectile_launch::actor_frame(world, actor, cell)?;
            let target = super::projectile_launch::actor_frame(world, target, cell)?;
            world
                .segment_clear(
                    cell,
                    source.position + Vec3::new(0., 0., source.height * (2. / 3.)),
                    target.position + Vec3::new(0., 0., target.height * (2. / 3.)),
                    Some(actor.0),
                )
                .map_err(|_| CastRejection::MissingAssets)?
        } else {
            world
                .geometry()
                .is_some_and(|region| region.cell(cell.0).is_some())
                || world.scene(cell).is_ok()
        },
        turning_to_target: world
            .body(actor)
            .is_ok_and(|body| body.server_turn().is_some()),
        manual_turning: world.body(actor).is_ok_and(|body| body.manual_turning()),
    })
}
pub(super) fn rejection(error: CastError) -> CastRejection {
    match error {
        CastError::Busy => CastRejection::Busy,
        CastError::StreakCooldown => CastRejection::StreakCooldown,
        CastError::Dead => CastRejection::Dead,
        CastError::PortalSpace => CastRejection::PortalSpace,
        CastError::WrongMode => CastRejection::WrongMode,
        CastError::InvalidTarget => CastRejection::InvalidTarget,
        CastError::OutOfRange => CastRejection::OutOfRange,
        CastError::Obstructed => CastRejection::Obstructed,
        _ => CastRejection::InvalidState,
    }
}
pub(super) fn validate_effect(effect: &SpellEffect) -> Result<(), CastRejection> {
    let enchantment = |spec: &bace_magic::EnchantmentSpec| -> Result<(), CastRejection> {
        if !spec.duration.is_finite() || spec.duration < 0.0 || !spec.value.is_finite() {
            return Err(CastRejection::InvalidState);
        }
        Ok(())
    };
    if let SpellEffect::LifeProjectile {
        proportion,
        damage_ratio,
        ..
    } = effect
        && (!proportion.is_finite()
            || !(0.0..=1.0).contains(proportion)
            || !damage_ratio.is_finite()
            || !(0.0..=1000.0).contains(damage_ratio))
    {
        return Err(CastRejection::InvalidState);
    }
    match effect {
        SpellEffect::Projectile(spec)
        | SpellEffect::LifeProjectile {
            projectile: spec, ..
        } => {
            if spec.template == 0
                || spec.count == 0
                || spec.count > 1024
                || !spec.radius.is_finite()
                || spec.radius <= 0.0
                || spec.radius > 20.0
                || !spec.speed.is_finite()
                || spec.speed <= 0.0
                || spec.speed > 1000.0
                || !spec.gravity.is_finite()
                || spec.gravity.abs() > 100.0
                || !spec.lifetime.is_finite()
                || spec.lifetime <= 0.0
                || spec.lifetime > 600.0
                || !spec.spread_degrees.is_finite()
                || !(0.0..=360.0).contains(&spec.spread_degrees)
                || !spec.padding.is_finite()
                || !spec.offset.is_finite()
                || spec.dimensions.contains(&0)
                || spec.dimensions.iter().any(|n| *n > 1024)
                || u64::from(spec.dimensions[0])
                    * u64::from(spec.dimensions[1])
                    * u64::from(spec.dimensions[2])
                    != u64::from(spec.count)
                || spec.minimum_damage > spec.maximum_damage
                || spec.maximum_damage > i32::MAX as u32
            {
                return Err(CastRejection::InvalidState);
            }
            if let Some(spec) = &spec.enchantment {
                enchantment(spec)?;
            }
        }
        SpellEffect::Enchantment(spec) | SpellEffect::FellowshipEnchantment(spec) => {
            enchantment(spec)?
        }
        SpellEffect::Transfer {
            proportion, loss, ..
        } => {
            if !proportion.is_finite()
                || !(0.0..=1.0).contains(proportion)
                || !loss.is_finite()
                || !(0.0..=1.0).contains(loss)
            {
                return Err(CastRejection::InvalidState);
            }
        }
        SpellEffect::Dispel(spec) | SpellEffect::FellowshipDispel(spec)
            if spec.minimum_power > spec.maximum_power =>
        {
            return Err(CastRejection::InvalidState);
        }
        _ => {}
    }
    Ok(())
}
/// Resource damage cannot inherit helpful permission merely from inconsistent
/// authored beneficial flags. GDLE's damage/transfer owners independently check
/// attack eligibility before modifying the victim.
pub(super) fn helpful_spell(spell: &PreparedSpell) -> bool {
    !spell.harmful
        && !matches!(
            spell.effect,
            SpellEffect::Boost {
                minimum: i32::MIN..=-1,
                ..
            } | SpellEffect::FellowshipBoost {
                minimum: i32::MIN..=-1,
                ..
            } | SpellEffect::Transfer {
                source_is_caster: false,
                ..
            } | SpellEffect::Projectile(_)
                | SpellEffect::LifeProjectile { .. }
        )
}
