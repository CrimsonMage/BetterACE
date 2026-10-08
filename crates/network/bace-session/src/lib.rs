//! Session lifecycle kernel; account verification and world entry are external.
//! This crate does not implement a playable stock-client login flow.
mod lifecycle;
mod registry;
pub use lifecycle::{SessionError, SessionLifecycle, SessionState};
pub use registry::{AdmissionError, RegisteredSession, SessionKey, SessionRegistry};
mod login;
pub use login::{LoginRejection, PasswordLogin, validate_password_login};
mod dispatch;
pub use dispatch::{DispatchError, DispatchedProgression, decode_progression};
mod accounts;
pub use accounts::{AccountAdmission, AccountCancellation, AccountSessionError, AccountSessions};
mod combat;
mod magic;
pub use combat::{DispatchedCombat, decode_combat};
pub use magic::{DispatchedMagic, decode_magic};
mod world_control;
pub use world_control::{DispatchedWorldControl, decode_world_control};
mod door;
pub use door::{DispatchedDoorUse, decode_door_use};

mod ui;
mod ui_option_map;
pub use ui::decode_ui;

mod inventory;
pub use inventory::{DispatchedInventory, decode_inventory};

mod training;
pub use training::decode_training;

mod crafting;
pub use crafting::{DispatchedCrafting, decode_crafting};

mod skill_devices;
pub use skill_devices::{
    DispatchedSkillDevice, SkillDeviceAction, SkillDeviceConfirmationType, decode_skill_device,
};

mod staff;
pub use staff::decode_staff_map;

mod social_actions;
pub use social_actions::{
    DispatchedSocial, SocialDispatch, decode_social_action, decode_turbine_social,
};

mod recall;
pub use recall::{DispatchedRecall, decode_recall};

mod locomotion;
pub use locomotion::{
    DispatchedLocomotion, LocomotionObservations, decode_locomotion, movement_epochs_match,
};

mod target_query;
pub use target_query::decode_target_query;
