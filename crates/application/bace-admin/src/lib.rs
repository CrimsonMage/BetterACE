//! Administrative command handling.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod auth;
mod chat_credentials;
mod chat_feed;
mod chat_http;
mod chat_listener;
mod console;
mod credentials;
mod http;
pub use chat_credentials::{
    ChatBotCredential, load_chat_bots, provision_chat_bots, validate_chat_bots,
};
pub use chat_feed::{
    ChatConsumer, ChatFeed, ChatPublisher, FeedBatch, FeedChannel, FeedError, FeedEvent,
};
pub use chat_http::{chat_router, serve_chat};
pub mod command_catalog;
mod command_parser;
mod shard_commands;
mod staff_operations;
pub use command_parser::{
    AuthorizedCommand, CommandError, ParsedCommand, authorize_command, parse_command,
};
pub use shard_commands::{PreparedShardCommand, ShardOperation, prepare_shard_command};
pub use staff_operations::{
    AccountOperation, BootSelector, Inspection, SecretArgument, StaffOperation, StaffTarget,
    TeleportOperation, prepare_staff_operation,
};

pub use console::{
    ContentAction, ContentChange, ContentPreview, ContentReply, ContentRequest, ContentRevision,
    ContentStatus, ControlAction, ControlRequest, HostConsole, HostStatus,
};
pub use credentials::{HostError, ensure_private_directory, load_operator, provision_operator};
pub use http::{console_router, serve_console};

mod command_compatibility;
pub use command_compatibility::{CommandCompatibility, command_compatibility};
