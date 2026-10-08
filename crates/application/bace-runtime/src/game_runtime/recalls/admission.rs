use super::*;
use crate::game_runtime::progression::ProgressionIngress;
impl GameRuntime {
    pub(in crate::game_runtime) fn handle_recall_message(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<ProgressionIngress, String> {
        let Some(session) = self.sessions.get(&key) else {
            return Ok(ProgressionIngress::Blocked);
        };
        let Some(loading) = &session.loading else {
            return Ok(ProgressionIngress::Unsupported);
        };
        let binding = loading.loaded.binding;
        if !self.players.entered(binding.actor) {
            return Ok(ProgressionIngress::Unsupported);
        }
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: message.sequence,
        };
        let dispatched =
            match decode_authenticated_recall(context, &message.bytes, self.limits.message_bytes) {
                Ok(v) => v,
                Err(bace_session::DispatchError::UnsupportedAction(_)) => {
                    return Ok(ProgressionIngress::Unsupported);
                }
                Err(bace_session::DispatchError::Wire(bace_wire::WireError::UnexpectedOpcode(
                    _,
                ))) => {
                    return Ok(ProgressionIngress::Unsupported);
                }
                Err(e) => return Err(format!("recall input: {e:?}")),
            };
        if self.recalls.pending.len() >= CAPACITY || self.recalls.pending.contains_key(&key) {
            return Ok(ProgressionIngress::Blocked);
        }
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("recall authenticated binding mismatch".into());
        }
        use bace_wire::RecallAction as A;
        let kind = match dispatched.request {
            A::Lifestone => RecallKind::Lifestone,
            A::House => RecallKind::House,
            A::Marketplace => RecallKind::Marketplace,
            A::AllegianceHometown => RecallKind::AllegianceHometown,
            A::AllegianceHousing => RecallKind::AllegianceHousing,
            A::PkArena => RecallKind::PkArena,
            A::PklArena => RecallKind::PklArena,
        };
        self.recalls.pending.insert(
            key,
            Pending {
                context: dispatched.context,
                binding,
                kind,
                name: loading.loaded.player.player.name.clone(),
                phase: Phase::Capture,
                prepared: None,
            },
        );
        Ok(ProgressionIngress::Accepted)
    }
}

fn decode_authenticated_recall(
    context: ActionContext,
    bytes: &[u8],
    maximum: usize,
) -> Result<bace_session::DispatchedRecall, bace_session::DispatchError> {
    let mut decoded = bace_session::decode_recall(
        bace_session::SessionState::WorldConnected,
        context,
        bytes,
        maximum,
    )?;
    // The session owner orders all gameplay by reliable-message sequence. The
    // independent client GameAction counter cannot advance that authority fence.
    decoded.context = context;
    Ok(decoded)
}
#[cfg(test)]
mod tests;
