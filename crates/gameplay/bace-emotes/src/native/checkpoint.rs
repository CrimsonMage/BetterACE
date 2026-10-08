//! Explicit bounded VM snapshot contract. Codec owners freeze their own versioned
//! DTO; this is not permission to serialize evolving manager internals.
use super::*;
#[derive(Clone, Debug, PartialEq)]
pub struct NativeScheduledRow {
    pub inline: bool,
    pub depth: u16,
    pub set: u32,
    pub action: u32,
    pub due: f64,
    pub order: u64,
    pub context: NpcContext,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NativePendingRow {
    pub ticket: u64,
    pub row: NativeScheduledRow,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NativeCheckpoint {
    pub order: u64,
    pub remaining: u32,
    pub work: Vec<NativeScheduledRow>,
    pub pending: Vec<NativePendingRow>,
    pub detached: Vec<u64>,
}
impl From<&Work> for NativeScheduledRow {
    fn from(w: &Work) -> Self {
        Self {
            inline: w.inline,
            depth: w.depth,
            set: w.set as u32,
            action: w.action as u32,
            due: w.due,
            order: w.order,
            context: w.context,
        }
    }
}
impl NativeEmoteManager {
    pub fn references_actor(&self, actor: bace_types::EntityId) -> bool {
        self.work
            .iter()
            .chain(self.pending.iter().map(|p| &p.work))
            .any(|w| w.context.source == actor || w.context.target == Some(actor))
    }

    /// A selected empty set still matters for Refuse precedence; it does not
    /// fall through into Give merely because it contains no actions.
    pub fn preview_selected_trigger(
        &self,
        trigger: &NativeTrigger,
        context: NpcContext,
        now: f64,
        draw: Option<f64>,
    ) -> Result<Option<NativeCheckpoint>, NativeError> {
        let Some(set) = self.program.select(trigger, draw)? else {
            return Ok(None);
        };
        let mut preview = Self::restore(self.program.clone(), self.checkpoint())?;
        if !preview.busy() && !preview.has_detached() {
            preview.remaining = preview.program.limits.instructions;
        }
        if !preview.program.sets[set].actions.is_empty() {
            preview.schedule(set, 0, context, now, 0, true)?;
        }
        Ok(Some(preview.checkpoint()))
    }

    pub fn restore_like(&self, checkpoint: NativeCheckpoint) -> Result<Self, NativeError> {
        Self::restore(self.program.clone(), checkpoint)
    }
    /// Select and schedule an invocation against immutable prepared content,
    /// without executing a row or calling an owner. Used for durable hand-ins.
    pub fn preview_trigger(
        &self,
        trigger: &NativeTrigger,
        context: NpcContext,
        now: f64,
        draw: Option<f64>,
    ) -> Result<Option<NativeCheckpoint>, NativeError> {
        let mut preview = Self::restore(self.program.clone(), self.checkpoint())?;
        let Some(set) = preview.program.select(trigger, draw)? else {
            return Ok(None);
        };
        if preview.program.sets[set].actions.is_empty() {
            return Ok(None);
        }
        preview.schedule(set, 0, context, now, 0, true)?;
        Ok(Some(preview.checkpoint()))
    }
    pub fn checkpoint(&self) -> NativeCheckpoint {
        NativeCheckpoint {
            detached: self.detached.clone(),
            order: self.order,
            remaining: self.remaining as u32,
            work: self.work.iter().map(NativeScheduledRow::from).collect(),
            pending: self
                .pending
                .iter()
                .map(|p| NativePendingRow {
                    ticket: p.ticket,
                    row: NativeScheduledRow::from(&p.work),
                })
                .collect(),
        }
    }
    pub fn restore(
        program: Arc<NativeProgram>,
        state: NativeCheckpoint,
    ) -> Result<Self, NativeError> {
        if state.detached.len() > program.limits.chains
            || state.detached.iter().enumerate().any(|(i, v)| {
                *v == 0
                    || state.detached[..i].contains(v)
                    || state.pending.iter().any(|p| p.ticket == *v)
            })
            || state.work.len() + state.pending.len() > program.limits.chains
            || state.remaining as usize > program.limits.instructions
        {
            return Err(NativeError::Capacity);
        }
        let validate = |r: &NativeScheduledRow| -> Result<(), NativeError> {
            let set = program
                .sets
                .get(r.set as usize)
                .ok_or(NativeError::InvalidContent)?;
            if r.depth as usize >= program.limits.chains
                || r.action as usize > set.actions.len()
                || !r.due.is_finite()
                || r.due < 0.0
                || r.order == 0
                || r.order > state.order
                || r.context.source.0 == 0
            {
                return Err(NativeError::InvalidContent);
            }
            Ok(())
        };
        for (index, row) in state.work.iter().enumerate() {
            validate(row)?;
            if state.work[..index].iter().any(|r| r.order == row.order)
                || state.pending.iter().any(|p| p.row.order == row.order)
            {
                return Err(NativeError::InvalidContent);
            }
        }
        for (index, p) in state.pending.iter().enumerate() {
            validate(&p.row)?;
            if p.ticket == 0
                || p.row.action as usize == program.sets[p.row.set as usize].actions.len()
                || state.pending[..index]
                    .iter()
                    .any(|old| old.ticket == p.ticket || old.row.order == p.row.order)
            {
                return Err(NativeError::InvalidContent);
            }
        }
        let work = |r: NativeScheduledRow| Work {
            inline: r.inline,
            depth: r.depth,
            set: r.set as usize,
            action: r.action as usize,
            due: r.due,
            order: r.order,
            context: r.context,
        };
        Ok(Self {
            detached: state.detached,
            program,
            work: state.work.into_iter().map(work).collect(),
            pending: state
                .pending
                .into_iter()
                .map(|p| Pending {
                    ticket: p.ticket,
                    work: work(p.row),
                })
                .collect(),
            order: state.order,
            remaining: state.remaining as usize,
        })
    }
    /// Exact owner completion is checked before changing the queue. Adapters use it
    /// to preflight durable adoption; it grants no mutation authority itself.
    pub fn pending_ticket(&self, ticket: u64) -> bool {
        self.pending.iter().any(|p| p.ticket == ticket)
    }
}
impl NativeEmoteManager {
    /// Source asynchronous owner work has zero post-delay here. This advances
    /// only scheduling state, with no owner read, random draw or gameplay effect.
    pub fn detach_pending_no_delay(&mut self, ticket: u64, now: f64) -> Result<(), NativeError> {
        self.detach_pending_without_execution(ticket, now, 0.0)
    }
    /// Freezeable source scheduling only; independently admitted owner work and
    /// its actual post-delay never execute another gameplay row in this method.
    pub fn detach_pending_without_execution(
        &mut self,
        ticket: u64,
        now: f64,
        post_delay: f64,
    ) -> Result<(), NativeError> {
        if !now.is_finite()
            || now < 0.0
            || !post_delay.is_finite()
            || post_delay < 0.0
            || !(now + post_delay).is_finite()
            || self.detached.len() >= self.program.limits.chains
            || self.detached.contains(&ticket)
        {
            return Err(NativeError::Capacity);
        }
        let index = self
            .pending
            .iter()
            .position(|p| p.ticket == ticket)
            .ok_or(NativeError::UnknownTicket)?;
        let w = &self.pending[index].work;
        let next = w.action + 1;
        if post_delay > 0.0 && self.order == u64::MAX {
            return Err(NativeError::Budget);
        }
        if next < self.program.sets[w.set].actions.len()
            && (!(now + post_delay + f64::from(self.program.sets[w.set].actions[next].delay))
                .is_finite()
                || self.order == u64::MAX)
        {
            return Err(NativeError::Time);
        }
        let pending = self.pending.swap_remove(index);
        let w = pending.work;
        if next < self.program.sets[w.set].actions.len() {
            self.schedule(
                w.set,
                next,
                w.context,
                now + post_delay,
                w.depth,
                post_delay == 0.0,
            )?;
        } else if post_delay > 0.0 {
            self.order = self.order.checked_add(1).ok_or(NativeError::Budget)?;
            self.work.push(Work {
                inline: false,
                depth: w.depth,
                set: w.set,
                action: next,
                context: w.context,
                due: now + post_delay,
                order: self.order,
            });
        }
        self.detached.push(ticket);
        Ok(())
    }
    pub fn preview_detachment_no_delay(
        &self,
        ticket: u64,
        now: f64,
    ) -> Result<NativeCheckpoint, NativeError> {
        let mut preview = Self::restore(self.program.clone(), self.checkpoint())?;
        preview.detach_pending_no_delay(ticket, now)?;
        Ok(preview.checkpoint())
    }
    pub fn preview_detachment(
        &self,
        ticket: u64,
        now: f64,
        post_delay: f64,
    ) -> Result<NativeCheckpoint, NativeError> {
        let mut preview = Self::restore(self.program.clone(), self.checkpoint())?;
        preview.detach_pending_without_execution(ticket, now, post_delay)?;
        Ok(preview.checkpoint())
    }
    pub fn has_immediate_invocation(&self, operation: u64) -> bool {
        self.work
            .iter()
            .any(|w| w.inline && w.context.operation == operation)
            || self
                .pending
                .iter()
                .any(|p| p.work.context.operation == operation)
    }
}
