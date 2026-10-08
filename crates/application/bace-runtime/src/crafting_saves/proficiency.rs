//! Compose source proficiency with recipe counters at one aggregate revision.
use super::*;
use bace_gameplay_api::{ProgressionProjection, ProgressionTarget, SkillAdvancement, TraitDetails};
use bace_simulation::{CraftingDecision, CraftingTicket};

pub fn join_proficiency(
    mut operation: PlacementOperation,
    player: &CraftingSavedPlayer,
    ticket: &CraftingTicket,
) -> Result<PlacementOperation, SaveCodecError> {
    let Some(patch) = &ticket.proficiency else {
        return Ok(operation);
    };
    let CraftingDecision::Tinker(recipe) = &ticket.decision else {
        return Err(invalid());
    };
    let change = &patch.change;
    let earned = &change.earned;
    let actor = player.saved.player.entity.object_id;
    let before_revision = player.saved.player.entity.mutation_revision;
    let actor_changed = recipe.actor_properties != recipe.actor_before_properties;
    if !recipe.success
        || actor != ticket.actor.0
        || actor != recipe.actor
        || before_revision != recipe.expected_actor_revision
        || crafting_properties(&player.saved.player.entity.state.properties)
            != recipe.actor_before_properties
        || change.additional_change != actor_changed
        || earned.experience.before_revision != before_revision
        || earned
            .experience
            .before_available
            .checked_sub(u64::from(change.spent))
            .and_then(|v| v.checked_add(earned.credited))
            != Some(earned.experience.after_available)
        || change.before.target != ProgressionTarget::Skill(change.usage.skill)
        || change.after.target != change.before.target
        || change.before.advancement != change.after.advancement
        || !matches!(
            change.before.advancement,
            SkillAdvancement::Trained | SkillAdvancement::Specialized
        )
        || change.before.experience_spent.checked_add(change.spent)
            != Some(change.after.experience_spent)
        || (!change.grants_experience && (change.spent != 0 || earned.credited != 0))
        || !operation.participants.contains(&actor)
        || patch.vitals.len() > 3
    {
        return Err(invalid());
    }
    let skill = player
        .saved
        .player
        .entity
        .state
        .properties
        .skills
        .iter()
        .find(|s| s.id as u32 == change.usage.skill)
        .ok_or_else(invalid)?;
    if skill_projection(skill)? != change.before {
        return Err(invalid());
    }
    let Some(TraitDetails::Skill {
        initial_level,
        resistance_at_last_check,
        last_used_time,
    }) = change.after.details
    else {
        return Err(invalid());
    };
    if !last_used_time.is_finite() || last_used_time != change.usage.unix_time {
        return Err(invalid());
    }
    // Verify the recipe row before replacing it. This catches conflicting joins
    // instead of silently dropping a concurrent player mutation from the batch.
    let mut recipe_player = player.saved.clone();
    apply_properties(
        &mut recipe_player.player.entity.state.properties,
        &recipe.actor_before_properties,
        &recipe.actor_properties,
        None,
    )?;
    recipe_player.player.entity.mutation_revision = recipe.actor_revision;
    let rows: Vec<_> = operation
        .snapshots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.object_id == actor)
        .collect();
    if rows.len() > 1 {
        return Err(invalid());
    }
    if let Some((_, row)) = rows.first() {
        if row.expected_version != player.persisted_version
            || row.mutation_revision != recipe.actor_revision
            || row.bytes != recipe_player.encode()?
        {
            return Err(invalid());
        }
    } else if actor_changed {
        return Err(invalid());
    }
    let row_index = rows.first().map(|(index, _)| *index);
    drop(rows);
    // Recipe actor deltas currently own only these two tinkering statistics.
    // XP/services/skills have their own validated proposal, never a scalar write.
    for key in recipe
        .actor_before_properties
        .keys()
        .chain(recipe.actor_properties.keys())
    {
        if recipe.actor_before_properties.get(key) != recipe.actor_properties.get(key)
            && !(key.kind == PropertyKind::Int && [205, 206].contains(&key.id))
        {
            return Err(invalid());
        }
    }
    let mut next = crate::npc_persistence::freeze_player_effect(
        &player.saved,
        &bace_simulation::NpcEffect::EarnedExperience {
            actor: ticket.actor,
            change: earned.clone(),
        },
    )?;
    apply_properties(
        &mut next.player.entity.state.properties,
        &recipe.actor_before_properties,
        &recipe.actor_properties,
        None,
    )?;
    let next_skill = next
        .player
        .entity
        .state
        .properties
        .skills
        .iter_mut()
        .find(|s| s.id == skill.id)
        .ok_or_else(invalid)?;
    next_skill.value = bace_content::Skill {
        level_from_pp: change.after.ranks,
        sac: change.after.advancement as u32,
        pp: change.after.experience_spent,
        init_level: initial_level,
        resistance_at_last_check,
        last_used_time,
    };
    if let Some(vitae) = &patch.vitae {
        if vitae.actor != ticket.actor
            || crate::player_death_state::restore_player_death_state(&player.saved)? != vitae.before
            || player.saved.enchantments
                != vitae
                    .before_enchantments
                    .iter()
                    .map(crate::enchantment_saves::freeze_enchantment)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| invalid())?
        {
            return Err(invalid());
        }
        crate::player_death_state::freeze_player_death_state(&mut next, &vitae.after)?;
        next.enchantments = vitae
            .registry
            .entries()
            .iter()
            .map(crate::enchantment_saves::freeze_enchantment)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| invalid())?;
    }
    let mut seen = BTreeSet::new();
    for vital in &patch.vitals {
        if vital.actor != ticket.actor || !seen.insert(vital.vital as u8) {
            return Err(invalid());
        }
        crate::portal_saves::apply_vital(&mut next, *vital)?;
    }
    let after_revision = earned.experience.after_revision;
    next.player.entity.mutation_revision = before_revision;
    if before_revision.checked_add(u64::from(next != player.saved)) != Some(after_revision) {
        return Err(invalid());
    }
    next.player.entity.mutation_revision = after_revision;
    let row = SaveSnapshot {
        object_id: actor,
        mutation_revision: after_revision,
        expected_version: player.persisted_version,
        bytes: next.encode()?,
    };
    if let Some(index) = row_index {
        operation.snapshots[index] = row;
    } else {
        operation.snapshots.push(row);
        operation.snapshots.sort_by_key(|s| s.object_id);
    }
    Ok(operation)
}
fn skill_projection(
    skill: &Property<bace_content::Skill, i32>,
) -> Result<ProgressionProjection, SaveCodecError> {
    Ok(ProgressionProjection {
        target: ProgressionTarget::Skill(u32::try_from(skill.id).map_err(|_| invalid())?),
        experience_spent: skill.value.pp,
        ranks: skill.value.level_from_pp,
        advancement: SkillAdvancement::try_from(skill.value.sac).map_err(|_| invalid())?,
        details: Some(TraitDetails::Skill {
            initial_level: skill.value.init_level,
            resistance_at_last_check: skill.value.resistance_at_last_check,
            last_used_time: skill.value.last_used_time,
        }),
    })
}

#[cfg(test)]
mod tests;
