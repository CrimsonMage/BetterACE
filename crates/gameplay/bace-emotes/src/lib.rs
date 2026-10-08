//! Content-defined actions and NPC scripting.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod signal;
pub use signal::{SignalCylinder, cylinder_distance};

mod script;
pub use script::{EmoteAction, EmoteEffect, EmoteError, EmoteScript, EmoteStep};

mod native;
mod native_actions;
mod native_queries;
pub use native::{
    NativeCheckpoint, NativeEmoteHost, NativeEmoteManager, NativeError, NativeLimits,
    NativePendingRow, NativeProgram, NativeScheduledRow, NativeStep, NativeTrigger, NpcActorFacts,
};
pub use native_actions::source_noop;
