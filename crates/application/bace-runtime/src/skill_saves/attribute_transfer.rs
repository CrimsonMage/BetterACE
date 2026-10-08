//! Frozen attribute starting-value mutation for one journaled character+device
//! operation. No proposed value is exposed as a committed player baseline.
use bace_character::AttributeTransferProposal;
use bace_content::Attribute;
use bace_gameplay_api::{ProgressionProjection, ProgressionTarget, TraitDetails};
use bace_storage_codec::PlayerSaveV6;

pub(super) fn freeze(
    saved: &PlayerSaveV6,
    proposal: AttributeTransferProposal,
) -> Result<PlayerSaveV6, super::SkillSaveError> {
    use super::SkillSaveError as E;
    saved.validate().map_err(E::Codec)?;
    if saved.player.entity.mutation_revision != proposal.expected_revision
        || proposal.expected_revision.checked_add(1) != Some(proposal.revision)
    {
        return Err(E::Identity);
    }
    let mut next = saved.clone();
    let attrs = &mut next.player.entity.state.properties.attributes;
    for (before, after) in [
        (proposal.from_before, proposal.from_after),
        (proposal.to_before, proposal.to_after),
    ] {
        let id = attribute_id(before)?;
        if attribute_id(after)? != id
            || before.experience_spent != after.experience_spent
            || before.ranks != after.ranks
            || before.advancement != after.advancement
        {
            return Err(E::Identity);
        }
        let record = attrs
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or(E::Identity)?;
        if record.value != attribute_value(before)? {
            return Err(E::Identity);
        }
        record.value = attribute_value(after)?;
    }
    next.player.entity.mutation_revision = proposal.revision;
    next.validate().map_err(E::Codec)?;
    Ok(next)
}
fn attribute_id(projection: ProgressionProjection) -> Result<u32, super::SkillSaveError> {
    let ProgressionTarget::Attribute(id) = projection.target else {
        return Err(super::SkillSaveError::Identity);
    };
    Ok(id as u32)
}
fn attribute_value(projection: ProgressionProjection) -> Result<Attribute, super::SkillSaveError> {
    let Some(TraitDetails::Attribute { starting_value }) = projection.details else {
        return Err(super::SkillSaveError::Identity);
    };
    Ok(Attribute {
        init_level: starting_value,
        level_from_cp: u32::from(projection.ranks),
        cp_spent: projection.experience_spent,
    })
}
