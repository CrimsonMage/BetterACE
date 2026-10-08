//! AC locomotion and motion tables.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod action_chain;
mod execution;
mod rate_model;
mod style_chain;
pub use action_chain::{
    ActionChainError, ActionChainRequest, MotionTableSource, ResolvedAction, ResolvedMotionPart,
    resolve_action, resolve_link, resolve_sequence,
};
pub use rate_model::{MotionClipRate, MotionRate, MotionRateModel};
mod intent;
mod locomotion;
mod root_motion;
pub use execution::{
    ExecutionClip, ExecutionCursor, MotionDomain, MotionExecutionEvent, MotionPlayback,
    MotionPlaybackEvent, MotionToken, PreparedMotionChain, SourceMotionState,
    SourceMotionTransition,
};
pub use locomotion::{
    LocomotionAxes, LocomotionControls, LocomotionProfile, MotionDrive, MotionPhysics,
};
pub use root_motion::{AnimatedLocomotion, RootCursor, RootCycle, RootFrame, RootSegment};
mod timeline;
mod turning;
pub use timeline::{
    MotionCursor, MotionHook, MotionHookPayload, MotionSegment, PreparedMotion, TimedMotionHook,
    TimelineError,
};
pub use turning::{TurnControl, TurnError, TurnIntent, heading_delta};

pub use intent::{Capabilities, MotionError, MotionIntent};

mod raw_locomotion;
pub use raw_locomotion::{RawLocomotion, interpret_raw_controls};

mod death;
pub use death::PreparedDeathMotion;
