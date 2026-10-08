use super::*;
pub(super) fn restore_monster_mode(attempt: &Attempt, world: &mut World) {
    if let Some(mode) = attempt.previous_mode
        && let Some(state) = world.combatant_mut(attempt.origin.actor())
    {
        state.set_mode(mode);
    }
}
impl Magic {
    pub(crate) fn register_monster_spellbook(
        &mut self,
        actor: EntityId,
        entries: Vec<bace_ai::MonsterSpell>,
        delay: f64,
    ) -> Result<(), CastRejection> {
        if self.busy(actor) {
            return Err(CastRejection::Busy);
        }
        if self.casters.get(&actor).is_none_or(|c| c.player) {
            return Err(CastRejection::InvalidState);
        }
        if entries.iter().any(|s| !self.spells.contains_key(&s.spell)) {
            return Err(CastRejection::MissingAssets);
        }
        let ai = bace_ai::MonsterSpellcasting::new(entries, delay)
            .map_err(|_| CastRejection::InvalidState)?;
        self.monster_ai.insert(actor, (ai, 0));
        Ok(())
    }
    pub(crate) fn monster_cast_request(
        &mut self,
        actor: EntityId,
        now: f64,
    ) -> Result<Option<(u64, u32)>, CastRejection> {
        if self.busy(actor) || !self.can_accept() {
            return Ok(None);
        }
        let Some((ai, sequence)) = self.monster_ai.get_mut(&actor) else {
            return Ok(None);
        };
        if !ai.ready(now) {
            return Ok(None);
        }
        let next = sequence.checked_add(1).ok_or(CastRejection::InvalidState)?;
        let mut identity = [0; 16];
        identity[..4].copy_from_slice(&actor.0.to_le_bytes());
        identity[4..12].copy_from_slice(&next.to_le_bytes());
        identity[15] = 0x41;
        let mut stream = self
            .random
            .as_ref()
            .ok_or(CastRejection::MissingAssets)?
            .event_stream(identity, Domain::Magic)
            .and_then(|stream| stream.fork(b"execution_epoch", self.execution_epoch))
            .map_err(|_| CastRejection::InvalidState)?;
        let draws: Vec<_> = (0..ai.entries().len())
            .map(|_| unit(&mut stream))
            .collect::<Result<_, _>>()?;
        let selected = ai
            .select(now, &draws)
            .map_err(|_| CastRejection::InvalidState)?;
        *sequence = next;
        Ok(selected.map(|spell| (next, spell)))
    }
}
