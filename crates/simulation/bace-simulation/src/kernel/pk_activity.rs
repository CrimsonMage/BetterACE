use super::*;
impl Kernel {
    pub fn pk_timer_active(&self, actor: EntityId) -> bool {
        self.world
            .combatant(actor)
            .is_some_and(|state| state.pk_activity_active(self.tick as f64 / 30.))
    }
}
