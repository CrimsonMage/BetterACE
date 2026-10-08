//! Durable-outcome output bridge; a pending proposal never produces success.
use bace_gameplay_api::CharacterBinding;
use bace_replication::{BatchLimits, EventSequencer, SessionBatch, SessionProjectionError};
use bace_simulation::{CraftingDecision, CraftingOutcome, CraftingResult};
use bace_wire::SalvageWireResult;

/// Empty salvage is a completed no-mutation operation. Nonempty salvage is
/// projected only from the simulation's matching durable receipt adoption.
/// Return None for quotes, pending/retried work, errors, rollback and tinkering;
/// their confirmation/text/property updates have separate prepared projections.
pub fn project_crafting_outcome(
    sequencer: &mut EventSequencer,
    binding: CharacterBinding,
    outcome: &CraftingOutcome,
    limits: BatchLimits,
) -> Result<Option<SessionBatch>, SessionProjectionError> {
    let proposal = match &outcome.result {
        Ok(CraftingResult::Committed(ticket)) => match &ticket.decision {
            CraftingDecision::Salvage(proposal) => proposal.as_ref(),
            CraftingDecision::Tinker(_) => return Ok(None),
        },
        Ok(CraftingResult::SalvageEmpty(proposal)) => {
            if !proposal.consumed.is_empty()
                || !proposal.bags.is_empty()
                || !proposal.results.is_empty()
            {
                return Err(SessionProjectionError::InvalidProjection);
            }
            proposal.as_ref()
        }
        _ => return Ok(None),
    };
    if proposal.actor != binding.actor.0
        || proposal.bags.len() != proposal.results.len()
        || proposal.bags.len() > 300
        || proposal.unsuitable.len() > 300
    {
        return Err(SessionProjectionError::InvalidProjection);
    }
    let mut bags = Vec::with_capacity(proposal.bags.len());
    for (bag, result) in proposal.bags.iter().zip(&proposal.results) {
        if result.material != bag.material
            || result.units != bag.units
            || result.workmanship != f64::from(bag.raw_workmanship) / f64::from(bag.num_items)
            || result.skill != proposal.reporting_skill
            || result.augmentation_bonus != proposal.augmentation_bonus
        {
            return Err(SessionProjectionError::InvalidProjection);
        }
        bags.push(SalvageWireResult {
            material: result.material,
            units: result.units,
            workmanship: result.workmanship,
        });
    }
    let unsuitable: Vec<_> = proposal.unsuitable.iter().map(|(id, _)| *id).collect();
    sequencer
        .project_salvage_results(
            binding,
            proposal.reporting_skill,
            &unsuitable,
            &bags,
            proposal.augmentation_bonus,
            limits,
        )
        .map(Some)
}
