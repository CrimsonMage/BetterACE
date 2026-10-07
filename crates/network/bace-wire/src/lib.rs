//! Checked codecs for the pinned official ACE wire protocol.
//! Primitives, framing, complete named identifiers and selected message families.
//! Identifier coverage does not imply payload or gameplay implementation.
mod checksum;
mod error;
mod framing;
mod isaac;
mod optional;
mod primitives;
pub use checksum::hash32;
pub use error::WireError;
pub use framing::*;
pub use isaac::{ClientKeys, Isaac};
pub use optional::OptionalHeaders;
pub use primitives::{Reader, Writer};
mod handshake;
pub use handshake::{ConnectRequest, decode_connect_response};
mod login;
pub use login::{LoginCredential, LoginRequest, NetAuthType};

pub mod opcode;
mod opcode_action;
mod opcode_character_error;
mod opcode_event;
mod opcode_group;
mod opcode_message;

mod envelope;
pub use envelope::{GameActionEnvelope, GameEventEnvelope};
mod character;
pub use character::{CharacterList, CharacterListEntry, CharacterReply};
mod chat;
pub use chat::{ChatMessage, ServerName};
mod control;
pub use control::AccountControl;
mod ddd;
pub use ddd::{DddBegin, DddControl, DddData, DddDatabase, DddIteration, DddRequestData};
mod property;
pub use property::{PropertyUpdate, PropertyValue};
mod progression;
pub use progression::{AttributeUpdate, CurrentVitalUpdate, SkillUpdate, VitalUpdate};
mod events;
pub use events::SimpleGameEvent;
mod ddd_response;
pub use ddd_response::{DddInterrogationResponse, DddIterationSet};
mod progression_request;
pub use progression_request::{ProgressionAction, ProgressionRequest};
mod character_create;
pub use character_create::{CharacterAbilities, CharacterAppearance, CharacterCreateRequest};
mod position;
pub use position::{
    AutonomousPositionOutput, MovementEpochs, PositionPack, PositionUpdate, VectorUpdate,
    WirePosition,
};
mod movement_input;
pub use movement_input::{
    ClientAutonomousPosition, ClientJump, ClientMoveToState, MotionCommandItem, RawMotionState,
};
mod motion_output;
pub use motion_output::{
    InterpretedMotion, MotionBody, MotionUpdate, MoveToParameters, TurnToParameters,
};
mod object_model;
pub use object_model::{ModelPalette, ModelPart, ModelTexture, ObjectModel};
mod restrictions;
pub use restrictions::{ObjectRestrictions, RestrictionPermission};
mod object_game;
pub use object_game::{ObjectGameData, ObjectGameOptions};
mod object_physics;
pub use object_physics::{
    PhysicsChild, PhysicsDescription, PhysicsMovement, PhysicsOptions, PhysicsParent,
    PhysicsSequences,
};
mod object;
pub use object::{AppearanceUpdate, ObjectCodecLimits, ObjectDescription};
mod object_control;
pub use motion_output::MovementDescription;
pub use object_control::ObjectControl;
mod turbine_chat;
pub use turbine_chat::{
    TurbineChatEvent, TurbineChatRequest, TurbineChatResponse, TurbineFrame, TurbineFrameHeader,
};
mod social_input;
pub use social_input::{SocialAction, SocialRequest};
mod social_tables;
pub use social_tables::{
    FriendEntry, FriendsUpdate, FriendsUpdateKind, SquelchDatabase, SquelchEntry, SquelchInfo,
};
mod social_output;
pub use social_output::{SocialCodecLimits, SocialEvent};
mod inventory_input;
pub use inventory_input::{InventoryAction, InventoryRequest, TradeAcceptance, VendorItemRequest};
mod inventory_output;
pub use inventory_output::{ContainerEntry, InventoryEvent};
mod vendor_output;
pub use vendor_output::{VendorCurrency, VendorListing, VendorListingItem};
