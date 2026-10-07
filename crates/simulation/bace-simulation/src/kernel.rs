use crate::{CharacterRegistrationError, characters::Characters};
use bace_character::CharacterProgression;
use bace_gameplay_api::{ActionContext, CharacterBinding, ProgressionOutcome, RaiseProgression};
use bace_geometry::Vec3;
use bace_motion::MotionIntent;
use bace_types::{CellId, EntityId};
use bace_world::{World, WorldError};
use std::collections::VecDeque;

pub enum Command {
    RaiseProgression {
        context: ActionContext,
        request: RaiseProgression,
    },
    Movement {
        actor: EntityId,
        epoch: u16,
        sequence: u32,
        intent: MotionIntent,
    },
    ServerTeleport {
        actor: EntityId,
        cell: CellId,
        position: Vec3,
    },
}

/// Pure, bounded, caller-driven simulation. Scheduling and clocks belong to
/// runtime; this function never waits for network, storage or a channel.
pub struct Kernel {
    world: World,
    commands: VecDeque<Command>,
    capacity: usize,
    tick: u64,
    characters: Characters,
    progression_outcomes: VecDeque<ProgressionOutcome>,
    outcome_capacity: usize,
}

impl Kernel {
    pub fn new(world: World, capacity: usize) -> Result<Self, SimulationError> {
        Self::with_gameplay_limits(world, capacity, capacity.min(4096), capacity)
    }
    pub fn with_gameplay_limits(
        world: World,
        capacity: usize,
        character_capacity: usize,
        outcome_capacity: usize,
    ) -> Result<Self, SimulationError> {
        if !(1..=65536).contains(&capacity)
            || !(1..=4096).contains(&character_capacity)
            || !(1..=65536).contains(&outcome_capacity)
        {
            return Err(SimulationError::Capacity);
        }
        Ok(Self {
            world,
            commands: VecDeque::new(),
            capacity,
            tick: 0,
            characters: Characters::new(character_capacity),
            progression_outcomes: VecDeque::with_capacity(outcome_capacity),
            outcome_capacity,
        })
    }
    /// Install only on the simulation owner after lifecycle ownership has been
    /// acquired. Failure returns the aggregate, preserving dirty state ownership.
    pub fn register_character(
        &mut self,
        binding: CharacterBinding,
        progression: CharacterProgression,
    ) -> Result<(), (CharacterRegistrationError, CharacterProgression)> {
        if self.world.body(binding.actor).is_err() {
            return Err((CharacterRegistrationError::MissingActor, progression));
        }
        self.characters.register(binding, progression)
    }
    /// Transfer the aggregate to its owning lifecycle/save service. This method
    /// does not assert that saves drained; callers must establish that separately.
    pub fn take_character(
        &mut self,
        binding: CharacterBinding,
    ) -> Result<CharacterProgression, CharacterRegistrationError> {
        self.characters.take(binding)
    }
    pub fn character(&self, actor: EntityId) -> Option<&CharacterProgression> {
        self.characters.get(actor)
    }
    pub fn take_progression_outcome(&mut self) -> Option<ProgressionOutcome> {
        self.progression_outcomes.pop_front()
    }
    pub fn pending_progression_outcomes(&self) -> usize {
        self.progression_outcomes.len()
    }
    pub fn queued_commands(&self) -> usize {
        self.commands.len()
    }
    pub fn has_queued_progression(&self) -> bool {
        self.commands
            .iter()
            .any(|command| matches!(command, Command::RaiseProgression { .. }))
    }
    pub fn progression_backpressured(&self) -> bool {
        self.progression_outcomes.len() == self.outcome_capacity
            && matches!(
                self.commands.front(),
                Some(Command::RaiseProgression { .. })
            )
    }
    pub fn has_characters(&self) -> bool {
        !self.characters.is_empty()
    }
    /// Nonblocking admission retains the exact command on bounded overload.
    pub fn try_enqueue(&mut self, command: Command) -> Result<(), Command> {
        if self.commands.len() >= self.capacity {
            return Err(command);
        }
        self.commands.push_back(command);
        Ok(())
    }
    pub fn enqueue(&mut self, command: Command) -> Result<(), SimulationError> {
        if self.commands.len() >= self.capacity {
            return Err(SimulationError::QueueFull);
        }
        self.commands.push_back(command);
        Ok(())
    }
    pub fn world(&self) -> &World {
        &self.world
    }
    pub fn ticks(&self) -> u64 {
        self.tick
    }
    pub fn step(&mut self) -> Result<Vec<String>, SimulationError> {
        let next_tick = self
            .tick
            .checked_add(1)
            .ok_or(SimulationError::TickOverflow)?;
        let mut rejected = Vec::new();
        while !self.commands.is_empty() {
            // Preserve FIFO and retain the request, with no mutation or consumed
            // sequence, until its correlated result has bounded output capacity.
            if self.progression_backpressured() {
                break;
            }
            let command = self.commands.pop_front().expect("nonempty queue");
            let result = match command {
                Command::RaiseProgression { context, request } => {
                    let outcome = self.characters.apply(
                        context,
                        request,
                        self.world.body(context.actor).is_ok(),
                    );
                    self.progression_outcomes.push_back(outcome);
                    Ok(())
                }
                Command::Movement {
                    actor,
                    epoch,
                    sequence,
                    intent,
                } => self.world.body_mut(actor).and_then(|body| {
                    body.submit_intent(epoch, sequence, intent)
                        .map_err(WorldError::from)
                }),
                Command::ServerTeleport {
                    actor,
                    cell,
                    position,
                } => self.world.teleport(actor, cell, position),
            };
            if let Err(error) = result {
                rejected.push(error.to_string());
            }
        }
        self.world.tick()?;
        self.tick = next_tick;
        Ok(rejected)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    #[error("command/outcome capacity must be 1..=65536 and character capacity 1..=4096")]
    Capacity,
    #[error("bounded command queue is full")]
    QueueFull,
    #[error("simulation tick counter exhausted")]
    TickOverflow,
    #[error(transparent)]
    World(#[from] WorldError),
}
