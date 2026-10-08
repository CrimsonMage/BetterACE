//! Bounded explicit-time NPC action sequencing, based on ACE EmoteManager's
//! ordered/delayed actions. Typed reward effects require durable acknowledgment.
//! Native emote category selection and numeric content adaptation are separate.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmoteAction {
    Say(String),
    DelayTicks(u64),
    GiveItem {
        template: u32,
        count: u32,
    },
    TakeItem {
        template: u32,
        count: u32,
    },
    AwardXp(u64),
    StampQuest(String),
    /// Predicate result is supplied from authoritative quest/item/stat state.
    Branch {
        predicate: u32,
        when_true: usize,
        when_false: usize,
    },
    End,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmoteEffect {
    Say(String),
    GiveItem { template: u32, count: u32 },
    TakeItem { template: u32, count: u32 },
    AwardXp(u64),
    StampQuest(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmoteStep {
    Waiting,
    Predicate { sequence: u64, predicate: u32 },
    Effect { sequence: u64, effect: EmoteEffect },
    Complete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmoteError {
    Capacity,
    InvalidAction,
    InvalidBranch,
    TimeOverflow,
    SequenceOverflow,
    AwaitingAcknowledgment,
    WrongAcknowledgment,
    Cancelled,
    Budget,
}

pub struct EmoteScript {
    actions: Vec<EmoteAction>,
    pc: usize,
    due: u64,
    sequence: u64,
    pending: bool,
    predicate_pending: bool,
    cancelled: bool,
    remaining: usize,
}
impl EmoteScript {
    pub fn new(actions: Vec<EmoteAction>, now: u64, max_steps: usize) -> Result<Self, EmoteError> {
        if actions.is_empty() || actions.len() > 1024 || !(1..=65536).contains(&max_steps) {
            return Err(EmoteError::Capacity);
        }
        for action in &actions {
            match action {
                EmoteAction::Say(text) if text.len() > 4096 => {
                    return Err(EmoteError::InvalidAction);
                }
                EmoteAction::StampQuest(name) if name.is_empty() || name.len() > 256 => {
                    return Err(EmoteError::InvalidAction);
                }
                EmoteAction::GiveItem { template, count }
                | EmoteAction::TakeItem { template, count }
                    if *template == 0 || *count == 0 =>
                {
                    return Err(EmoteError::InvalidAction);
                }
                EmoteAction::Branch {
                    when_true,
                    when_false,
                    ..
                } if *when_true >= actions.len() || *when_false >= actions.len() => {
                    return Err(EmoteError::InvalidBranch);
                }
                _ => {}
            }
        }
        Ok(Self {
            actions,
            pc: 0,
            due: now,
            sequence: 0,
            pending: false,
            predicate_pending: false,
            cancelled: false,
            remaining: max_steps,
        })
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
    /// At most one action per call. Effects remain pending until acknowledged;
    /// retrying does not repeat them. Durable effects require commit confirmation.
    pub fn step(&mut self, now: u64) -> Result<EmoteStep, EmoteError> {
        if self.cancelled {
            return Err(EmoteError::Cancelled);
        }
        if self.pending || self.predicate_pending {
            return Err(EmoteError::AwaitingAcknowledgment);
        }
        if now < self.due {
            return Ok(EmoteStep::Waiting);
        }
        let Some(action) = self.actions.get(self.pc) else {
            return Ok(EmoteStep::Complete);
        };
        if matches!(action, EmoteAction::End) {
            return Ok(EmoteStep::Complete);
        }
        if self.remaining == 0 {
            self.cancelled = true;
            return Err(EmoteError::Budget);
        }
        if let EmoteAction::Branch { predicate, .. } = action {
            self.sequence = self
                .sequence
                .checked_add(1)
                .ok_or(EmoteError::SequenceOverflow)?;
            self.predicate_pending = true;
            return Ok(EmoteStep::Predicate {
                sequence: self.sequence,
                predicate: *predicate,
            });
        }
        if let EmoteAction::DelayTicks(delay) = action {
            let due = now.checked_add(*delay).ok_or(EmoteError::TimeOverflow)?;
            self.due = due;
            self.pc += 1;
            self.remaining -= 1;
            return Ok(EmoteStep::Waiting);
        }
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(EmoteError::SequenceOverflow)?;
        let effect = match action {
            EmoteAction::Say(text) => EmoteEffect::Say(text.clone()),
            EmoteAction::GiveItem { template, count } => EmoteEffect::GiveItem {
                template: *template,
                count: *count,
            },
            EmoteAction::TakeItem { template, count } => EmoteEffect::TakeItem {
                template: *template,
                count: *count,
            },
            EmoteAction::AwardXp(xp) => EmoteEffect::AwardXp(*xp),
            EmoteAction::StampQuest(name) => EmoteEffect::StampQuest(name.clone()),
            _ => unreachable!("control actions handled above"),
        };
        self.sequence = sequence;
        self.pending = true;
        self.remaining -= 1;
        Ok(EmoteStep::Effect { sequence, effect })
    }
    pub fn resolve_predicate(
        &mut self,
        sequence: u64,
        predicate: u32,
        result: bool,
    ) -> Result<(), EmoteError> {
        if self.cancelled {
            return Err(EmoteError::Cancelled);
        }
        if self.pending {
            return Err(EmoteError::AwaitingAcknowledgment);
        }
        let Some(EmoteAction::Branch {
            predicate: expected,
            when_true,
            when_false,
        }) = self.actions.get(self.pc)
        else {
            return Err(EmoteError::InvalidBranch);
        };
        if !self.predicate_pending || sequence != self.sequence || predicate != *expected {
            return Err(EmoteError::InvalidBranch);
        }
        if self.remaining == 0 {
            self.cancelled = true;
            return Err(EmoteError::Budget);
        }
        self.predicate_pending = false;
        self.pc = if result { *when_true } else { *when_false };
        self.remaining -= 1;
        Ok(())
    }
    pub fn acknowledge(&mut self, sequence: u64, succeeded: bool) -> Result<(), EmoteError> {
        if !self.pending || sequence != self.sequence {
            return Err(EmoteError::WrongAcknowledgment);
        }
        self.pending = false;
        if succeeded {
            self.pc += 1;
        } else {
            self.cancelled = true;
        }
        Ok(())
    }
}
