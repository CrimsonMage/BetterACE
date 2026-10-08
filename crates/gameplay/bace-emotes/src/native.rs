//! Pinned native EmoteManager category selection and explicit-time execution.
mod checkpoint;
use bace_content::{Emote, EmoteAction};
use bace_gameplay_api::{
    NpcCompletion, NpcContext, NpcFailure, NpcOperation, NpcQuery, NpcQueryValue,
};
pub use checkpoint::{NativeCheckpoint, NativePendingRow, NativeScheduledRow};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NpcActorFacts {
    pub player: bool,
    pub creature: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NativeTrigger {
    pub category: u32,
    pub quest: Option<String>,
    pub vendor: Option<i32>,
    pub template: Option<u32>,
    pub style: Option<u32>,
    pub motion: Option<u32>,
    pub health_fraction: Option<f32>,
    pub random: bool,
}
impl Default for NativeTrigger {
    fn default() -> Self {
        Self {
            category: 0,
            quest: None,
            vendor: None,
            template: None,
            style: None,
            motion: None,
            health_fraction: None,
            random: true,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct NativeLimits {
    pub sets: usize,
    pub actions: usize,
    pub chains: usize,
    pub instructions: usize,
}
impl Default for NativeLimits {
    fn default() -> Self {
        Self {
            sets: 4096,
            actions: 65536,
            chains: 76,
            instructions: 65536,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeError {
    InvalidContent,
    Capacity,
    Budget,
    Time,
    Busy,
    UnknownTicket,
    Owner(NpcFailure),
}
impl From<NpcFailure> for NativeError {
    fn from(e: NpcFailure) -> Self {
        Self::Owner(e)
    }
}
#[derive(Clone, Debug)]
pub struct NativeProgram {
    pub(crate) sets: Vec<Emote>,
    limits: NativeLimits,
}
/// This port never grants success merely for emitting an effect. A mutable owner
/// applies a change or returns Pending until its real/durable completion arrives.
pub trait NativeEmoteHost {
    fn facts(&self, actor: bace_types::EntityId) -> Option<NpcActorFacts>;
    fn draw(&mut self) -> Result<f64, NpcFailure>;
    fn query(&mut self, context: NpcContext, query: NpcQuery) -> Result<NpcQueryValue, NpcFailure>;
    fn execute(
        &mut self,
        context: NpcContext,
        operation: NpcOperation,
    ) -> Result<NpcCompletion, NpcFailure>;
    fn text(
        &mut self,
        context: NpcContext,
        message: &str,
        quest: Option<&str>,
    ) -> Result<String, NpcFailure>;
}
pub(crate) fn name_eq(a: &str, b: &str) -> bool {
    fn folded(s: &str) -> impl Iterator<Item = char> + '_ {
        s.chars().map(|c| {
            // OrdinalIgnoreCase does not fold dotless i or long s into ASCII.
            if matches!(c, '\u{0131}' | '\u{017f}') {
                return c;
            }
            let mut uppercase = c.to_uppercase();
            let first = uppercase.next().unwrap_or(c);
            if uppercase.next().is_none() { first } else { c }
        })
    }
    folded(a).eq(folded(b))
}
impl NativeProgram {
    pub fn prepare(sets: Vec<Emote>, limits: NativeLimits) -> Result<Self, NativeError> {
        if limits.sets == 0
            || limits.sets > 4096
            || limits.actions == 0
            || limits.actions > 65536
            || limits.chains == 0
            || limits.chains > 76
            || limits.instructions == 0
            || limits.instructions > 65536
            || sets.len() > limits.sets
        {
            return Err(NativeError::Capacity);
        }
        let mut count = 0usize;
        for set in &sets {
            count = count
                .checked_add(set.actions.len())
                .ok_or(NativeError::Capacity)?;
            if count > limits.actions {
                return Err(NativeError::Capacity);
            }
            if !(0..=38).contains(&set.category)
                || !set.probability.is_finite()
                || !(0.0..=1.0).contains(&set.probability)
                || set.quest.as_ref().is_some_and(|q| q.len() > 4096)
            {
                return Err(NativeError::InvalidContent);
            }
            for a in &set.actions {
                if !crate::native_actions::known(a.r#type)
                    || !a.delay.is_finite()
                    || a.delay < 0.0
                    || a.delay > 86400.0
                    || !a.extent.is_finite()
                    || [&a.message, &a.test_string]
                        .iter()
                        .any(|s| s.as_ref().is_some_and(|s| s.len() > 65536))
                {
                    return Err(NativeError::InvalidContent);
                }
            }
        }
        Ok(Self { sets, limits })
    }
    pub fn select(
        &self,
        trigger: &NativeTrigger,
        draw: Option<f64>,
    ) -> Result<Option<usize>, NativeError> {
        if trigger.category > 38 {
            return Err(NativeError::InvalidContent);
        }
        if trigger.random && draw.is_none_or(|r| !r.is_finite() || !(0.0..1.0).contains(&r)) {
            return Err(NativeError::InvalidContent);
        }
        let mut result = None;
        for (index, set) in self.sets.iter().enumerate() {
            if set.category as u32 != trigger.category {
                continue;
            }
            if let Some(quest) = &trigger.quest {
                let matched = match &set.quest {
                    Some(q) => name_eq(q, quest),
                    None => matches!(trigger.category, 24 | 38),
                };
                if !matched {
                    continue;
                }
            }
            if trigger.vendor.is_some() && set.vendor_type != trigger.vendor {
                continue;
            }
            if trigger.template.is_some() && set.weenie_class_id != trigger.template {
                continue;
            }
            if trigger.category == 5
                && (set.style.is_some() && set.style != trigger.style
                    || set.substyle.is_some() && set.substyle != trigger.motion)
            {
                continue;
            }
            if trigger.category == 15
                && let Some(health) = trigger.health_fraction
                && (!health.is_finite()
                    || set.min_health.is_none_or(|v| health < v)
                    || set.max_health.is_none_or(|v| health > v))
            {
                continue;
            }
            if trigger.random {
                if f64::from(set.probability) <= draw.expect("validated draw") {
                    continue;
                }
                if result.is_none_or(|old: usize| set.probability < self.sets[old].probability) {
                    result = Some(index);
                }
            } else {
                return Ok(Some(index));
            }
        }
        Ok(result)
    }
    pub fn set_count(&self) -> usize {
        self.sets.len()
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NativeStep {
    Idle,
    Waiting,
    Executed { action: u32 },
    SourceNoop { action: u32 },
    Pending { ticket: u64 },
    Completed,
}
struct Work {
    inline: bool,
    depth: u16,
    set: usize,
    action: usize,
    due: f64,
    order: u64,
    context: NpcContext,
}
struct Pending {
    ticket: u64,
    work: Work,
}
pub struct NativeEmoteManager {
    program: Arc<NativeProgram>,
    work: Vec<Work>,
    pending: Vec<Pending>,
    detached: Vec<u64>,
    order: u64,
    remaining: usize,
}
impl NativeEmoteManager {
    pub fn new(program: Arc<NativeProgram>) -> Self {
        let capacity = program.limits.chains;
        Self {
            remaining: program.limits.instructions,
            program,
            work: Vec::with_capacity(capacity),
            pending: Vec::with_capacity(capacity),
            detached: Vec::with_capacity(capacity),
            order: 0,
        }
    }
    pub fn busy(&self) -> bool {
        !self.work.is_empty() || !self.pending.is_empty()
    }
    pub fn trigger(
        &mut self,
        trigger: &NativeTrigger,
        context: NpcContext,
        now: f64,
        host: &mut impl NativeEmoteHost,
    ) -> Result<bool, NativeError> {
        if self.busy() {
            return Err(NativeError::Busy);
        }
        self.remaining = self.program.limits.instructions;
        self.enqueue_trigger(trigger, context, now, 0, host)
    }
    fn enqueue_trigger(
        &mut self,
        trigger: &NativeTrigger,
        context: NpcContext,
        now: f64,
        depth: u16,
        host: &mut impl NativeEmoteHost,
    ) -> Result<bool, NativeError> {
        if self.work.len() + self.pending.len() >= self.program.limits.chains {
            return Err(NativeError::Capacity);
        }
        let draw = if trigger.random {
            Some(host.draw()?)
        } else {
            None
        };
        let Some(set) = self.program.select(trigger, draw)? else {
            return Ok(false);
        };
        if self.program.sets[set].actions.is_empty() {
            return Ok(false);
        }
        self.schedule(set, 0, context, now, depth, true)?;
        Ok(true)
    }
    fn schedule(
        &mut self,
        set: usize,
        action: usize,
        context: NpcContext,
        now: f64,
        depth: u16,
        inline_allowed: bool,
    ) -> Result<(), NativeError> {
        if depth as usize >= self.program.limits.chains {
            return Err(NativeError::Capacity);
        }
        if self.work.len() + self.pending.len() >= self.program.limits.chains {
            return Err(NativeError::Capacity);
        }
        let due = now + f64::from(self.program.sets[set].actions[action].delay);
        if !now.is_finite() || now < 0.0 || !due.is_finite() {
            return Err(NativeError::Time);
        }
        let order = self.order.checked_add(1).ok_or(NativeError::Budget)?;
        self.order = order;
        self.work.push(Work {
            inline: inline_allowed && due == now,
            depth,
            set,
            action,
            due,
            order,
            context,
        });
        Ok(())
    }
    pub fn step(
        &mut self,
        now: f64,
        host: &mut impl NativeEmoteHost,
    ) -> Result<NativeStep, NativeError> {
        if !now.is_finite() || now < 0.0 {
            return Err(NativeError::Time);
        }
        let Some(index) = self
            .work
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                b.inline.cmp(&a.inline).then_with(|| {
                    if a.inline && b.inline {
                        b.depth.cmp(&a.depth).then(a.order.cmp(&b.order))
                    } else {
                        a.due.total_cmp(&b.due).then(a.order.cmp(&b.order))
                    }
                })
            })
            .map(|(i, _)| i)
        else {
            return Ok(if self.pending.is_empty() {
                NativeStep::Idle
            } else {
                NativeStep::Waiting
            });
        };
        if self.work[index].due > now {
            return Ok(NativeStep::Waiting);
        }
        if self.remaining == 0 {
            return Err(NativeError::Budget);
        }
        if self.work[index].action == self.program.sets[self.work[index].set].actions.len() {
            self.work.swap_remove(index);
            return Ok(NativeStep::Completed);
        }
        if self.work.len() + self.pending.len() >= self.program.limits.chains
            && self.work[index].action + 1 < self.program.sets[self.work[index].set].actions.len()
            && crate::native_actions::branches(
                self.program.sets[self.work[index].set].actions[self.work[index].action].r#type,
            )
        {
            return Err(NativeError::Capacity);
        }
        let work = self.work.swap_remove(index);
        let set = &self.program.sets[work.set];
        let action = &set.actions[work.action];
        let code = action.r#type;
        let outcome =
            crate::native_actions::execute(&self.program, set, action, work.context, host);
        let (completion, source_noop) = match outcome {
            Ok(v) => v,
            Err(NativeError::Owner(NpcFailure::Capacity | NpcFailure::DurabilityPending)) => {
                self.work.push(work);
                return Ok(NativeStep::Waiting);
            }
            Err(e) => {
                self.work.push(work);
                return Err(e);
            }
        };
        self.remaining -= 1;
        if let NpcCompletion::Detached { ticket, post_delay } = completion {
            if ticket == 0
                || self.detached.len() >= self.program.limits.chains
                || self.detached.contains(&ticket)
                || !post_delay.is_finite()
                || post_delay < 0.0
            {
                self.work.push(work);
                return Err(NativeError::Capacity);
            }
            self.detached.push(ticket);
            self.finish(work, NpcCompletion::Applied { post_delay }, now, host)?;
            return Ok(NativeStep::Pending { ticket });
        }
        if let NpcCompletion::Pending { ticket } = completion {
            if self.pending.iter().any(|p| p.ticket == ticket) {
                self.work.push(work);
                return Err(NativeError::UnknownTicket);
            }
            self.pending.push(Pending { ticket, work });
            return Ok(NativeStep::Pending { ticket });
        }
        self.finish(work, completion, now, host)?;
        Ok(if source_noop {
            NativeStep::SourceNoop { action: code }
        } else {
            NativeStep::Executed { action: code }
        })
    }
    fn finish(
        &mut self,
        work: Work,
        completion: NpcCompletion,
        now: f64,
        host: &mut impl NativeEmoteHost,
    ) -> Result<(), NativeError> {
        let action = &self.program.sets[work.set].actions[work.action];
        if let NpcCompletion::Branch { category } = completion {
            let trigger = NativeTrigger {
                category,
                quest: action.message.clone(),
                random: true,
                ..Default::default()
            };
            self.enqueue_trigger(
                &trigger,
                work.context,
                now,
                work.depth.checked_add(1).ok_or(NativeError::Budget)?,
                host,
            )?;
        }
        let delay = match completion {
            NpcCompletion::Applied { post_delay } => post_delay,
            _ => 0.0,
        };
        if !delay.is_finite() || delay < 0.0 {
            return Err(NativeError::Time);
        }
        if work.action + 1 < self.program.sets[work.set].actions.len() {
            self.schedule(
                work.set,
                work.action + 1,
                work.context,
                now + delay,
                work.depth,
                delay == 0.0,
            )?;
        } else if delay > 0.0 {
            let due = now + delay;
            if !due.is_finite() {
                return Err(NativeError::Time);
            }
            self.order = self.order.checked_add(1).ok_or(NativeError::Budget)?;
            self.work.push(Work {
                inline: false,
                depth: work.depth,
                set: work.set,
                action: work.action + 1,
                context: work.context,
                due,
                order: self.order,
            });
        }
        Ok(())
    }
    pub fn complete(
        &mut self,
        ticket: u64,
        completion: NpcCompletion,
        now: f64,
        host: &mut impl NativeEmoteHost,
    ) -> Result<(), NativeError> {
        if matches!(
            completion,
            NpcCompletion::Pending { .. } | NpcCompletion::Detached { .. }
        ) {
            return Err(NativeError::UnknownTicket);
        }
        if !now.is_finite()
            || now < 0.0
            || matches!(completion,NpcCompletion::Applied{post_delay} if !post_delay.is_finite()||post_delay<0.0)
        {
            return Err(NativeError::Time);
        }
        if self.work.len() + self.pending.len() >= self.program.limits.chains
            && matches!(completion, NpcCompletion::Branch { .. })
        {
            return Err(NativeError::Capacity);
        }
        let index = self
            .pending
            .iter()
            .position(|p| p.ticket == ticket)
            .ok_or(NativeError::UnknownTicket)?;
        let pending = self.pending.swap_remove(index);
        self.finish(pending.work, completion, now, host)
    }
    pub fn program(&self) -> &Arc<NativeProgram> {
        &self.program
    }
    pub fn detach_pending(
        &mut self,
        ticket: u64,
        post_delay: f64,
        now: f64,
        host: &mut impl NativeEmoteHost,
    ) -> Result<(), NativeError> {
        if !now.is_finite()
            || now < 0.0
            || !post_delay.is_finite()
            || post_delay < 0.0
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
        let pending = self.pending.swap_remove(index);
        self.detached.push(ticket);
        self.finish(
            pending.work,
            NpcCompletion::Applied { post_delay },
            now,
            host,
        )
    }
    pub fn has_detached(&self) -> bool {
        !self.detached.is_empty()
    }
    pub fn complete_detached(&mut self, ticket: u64) -> Result<(), NativeError> {
        let index = self
            .detached
            .iter()
            .position(|v| *v == ticket)
            .ok_or(NativeError::UnknownTicket)?;
        self.detached.swap_remove(index);
        Ok(())
    }
    pub fn cancel(&mut self) {
        self.work.clear();
        self.pending.clear();
    }
}
pub(crate) fn text(a: &EmoteAction) -> String {
    a.message.clone().unwrap_or_default()
}
