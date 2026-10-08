//! Training decode is separate from XP expenditure; neither grants authority.
use bace_gameplay_api::{ActionContext, TrainSkill};
use bace_wire::{GameActionEnvelope, ProgressionAction, ProgressionRequest, WireError};
pub fn decode_training(
    bytes: &[u8],
    mut context: ActionContext,
    max_bytes: usize,
) -> Result<(ActionContext, TrainSkill), WireError> {
    let envelope = GameActionEnvelope::decode(bytes, max_bytes)?;
    let request = ProgressionRequest::decode(envelope.action, envelope.payload, max_bytes)?;
    let ProgressionAction::TrainSkill {
        skill,
        credits_spent,
    } = request.action
    else {
        return Err(WireError::InvalidEncoding);
    };
    context.sequence = envelope.sequence;
    Ok((
        context,
        TrainSkill {
            skill,
            quoted_credits: credits_spent,
        },
    ))
}
