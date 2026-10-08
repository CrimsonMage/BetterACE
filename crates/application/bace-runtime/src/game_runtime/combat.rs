//! Authenticated physical intent and retained owner output. Ammunition uses the
//! valuable-operation lane; an attack request itself never acknowledges a save.
mod output;
mod projectiles;
mod resources;
use super::*;
use bace_gameplay_api::{ActionContext, CombatOutcome};
use bace_types::EntityId;
pub(super) struct CombatRuntime {
    pending: BTreeMap<SessionKey, ActionContext>,
    outcome: Option<CombatOutcome>,
    event: Option<bace_simulation::CombatEvent>,
    physical: Option<bace_simulation::PhysicalCombatEvent>,
    physical_recipient: u8,
    projectiles: projectiles::Projectiles,
    resources: resources::Resources,
}
impl CombatRuntime {
    pub(super) fn new() -> Self {
        Self {
            pending: BTreeMap::new(),
            outcome: None,
            event: None,
            physical: None,
            physical_recipient: 0,
            projectiles: projectiles::Projectiles::new(),
            resources: resources::Resources::new(),
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
            || self.outcome.is_some()
            || self.event.is_some()
            || self.physical.is_some()
            || self.resources.has_pending()
            || self.projectiles.has_pending()
    }
}
impl GameRuntime {
    pub(super) fn combat_ingress_blocked(&self, key: SessionKey) -> bool {
        self.combat.pending.contains_key(&key)
            || self
                .players
                .binding_for(key)
                .is_some_and(|b| self.combat.resources.owns(b.actor))
    }
    pub(super) fn handle_combat_message(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<movement::MovementIngress, String> {
        use movement::MovementIngress as I;
        let envelope = match bace_wire::GameActionEnvelope::decode(
            &message.bytes,
            self.limits.message_bytes,
        ) {
            Ok(value) => value,
            Err(_) => return Ok(I::Unsupported),
        };
        use bace_wire::opcode::GameActionType as A;
        if !matches!(
            envelope.action,
            A::TargetedMeleeAttack
                | A::TargetedMissileAttack
                | A::ChangeCombatMode
                | A::CancelAttack
        ) {
            return Ok(I::Unsupported);
        }
        if self.combat.pending.contains_key(&key) {
            return Ok(I::Blocked);
        }
        let Some(binding) = self.players.binding_for(key) else {
            return Ok(I::Accepted);
        };
        if !self.players.entered(binding.actor) {
            return Ok(I::Blocked);
        }
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: message.sequence,
        };
        let request = match bace_session::decode_combat(
            bace_session::SessionState::WorldConnected,
            context,
            &message.bytes,
            self.limits.message_bytes,
        ) {
            Ok(value) => value.request,
            Err(bace_session::DispatchError::Wire(_)) => {
                self.sessions
                    .get_mut(&key)
                    .ok_or("combat session missing")?
                    .terminated = true;
                return Ok(I::Accepted);
            }
            Err(error) => return Err(format!("combat dispatch: {error:?}")),
        };
        // The accepted reliable sequence shares the simulation action fence with
        // other families. The independent client GameAction counter is untrusted.
        match self
            .simulation
            .input()
            .try_submit(bace_simulation::Command::Combat { context, request })
        {
            Ok(()) => {
                self.combat.pending.insert(key, context);
                Ok(I::Accepted)
            }
            Err(std::sync::mpsc::TrySendError::Full(_)) => Ok(I::Blocked),
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                Err("combat owner closed; input retained".into())
            }
        }
    }
    pub(super) fn poll_combat(&mut self, unix: u64) -> Result<(), String> {
        self.poll_physical_resources(unix)?;
        self.poll_combat_output()
    }
}
