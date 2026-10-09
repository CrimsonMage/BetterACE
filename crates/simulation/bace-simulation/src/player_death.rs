//! Player death metadata and immutable durable work. World and Magic retain sole
//! ownership of bodies, vitals and enchantment registries.
use bace_types::EntityId;
use std::collections::{BTreeMap, VecDeque};
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerDeathState {
    pub num_deaths: u32,
    pub death_level: u32,
    pub vitae_pool: i32,
    pub pk_status: u32,
    pub pk_respite_elapsed: Option<f64>,
    pub protection_elapsed: Option<f64>,
    /// ACE PositionType.LastOutsideDeath, owned with death metadata.
    pub last_outside_death: Option<bace_interactions::PortalPosition>,
    pub olthoi_loot_timestamp: Option<i32>,
}
impl Default for PlayerDeathState {
    fn default() -> Self {
        Self {
            num_deaths: 0,
            death_level: 1,
            vitae_pool: 0,
            pk_status: 2,
            pk_respite_elapsed: None,
            protection_elapsed: None,
            last_outside_death: None,
            olthoi_loot_timestamp: None,
        }
    }
}
impl PlayerDeathState {
    pub fn validate(&self) -> Result<(), PlayerDeathError> {
        if self.olthoi_loot_timestamp.is_some_and(|t| t < 0)
            || self.death_level == 0
            || self.death_level > 275
            || self.vitae_pool < 0
            || self.pk_status > 127
            || self.num_deaths > i32::MAX as u32
            || self
                .pk_respite_elapsed
                .is_some_and(|t| !t.is_finite() || t < 0.)
            || self
                .protection_elapsed
                .is_some_and(|t| !t.is_finite() || !(0.0..=60.0).contains(&t))
            || self
                .last_outside_death
                .is_some_and(|p| p.validate().is_err())
        {
            return Err(PlayerDeathError::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerDeathError {
    Invalid,
    Busy,
    Capacity,
    MissingAssets,
    Stale,
    Receipt,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerVitaeRecovery {
    pub actor: EntityId,
    pub before: PlayerDeathState,
    pub after: PlayerDeathState,
    pub before_enchantments: Vec<bace_magic::EnchantmentEntry>,
    pub registry: bace_magic::VitaeMutation,
}
pub(crate) struct PlayerDeaths {
    pub(crate) states: BTreeMap<EntityId, PlayerDeathState>,
    pub(crate) pending: BTreeMap<EntityId, PendingDeath>,
    pub(crate) events: VecDeque<PlayerDeathEvent>,
    pub(crate) outcomes: VecDeque<crate::PlayerDeathServiceOutcome>,
    pub(crate) capacity: usize,
    pub(crate) next: u64,
    pub(crate) random: Option<std::sync::Arc<bace_random::RandomRoot>>,
    pub(crate) epoch: u64,
    pub(crate) timers: BTreeMap<EntityId, u64>,
    pub(crate) timer_scratch: Vec<EntityId>,
    pub(crate) corpse_access: BTreeMap<EntityId, CorpseAccessState>,
    pub(crate) corpse_access_outcomes: VecDeque<CorpseAccessOutcome>,
    /// Recipient → granting player → source one-hour Unix expiry.
    pub(crate) consent_grants: BTreeMap<EntityId, BTreeMap<EntityId, ConsentGrant>>,
    pub(crate) consent_outcomes: VecDeque<CorpseConsentOutcome>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConsentGrant {
    pub(crate) expires_at: u64,
    pub(crate) granter_name: String,
}

#[derive(Clone, Debug)]
pub struct CorpseConsentCommand {
    pub correlation: u64,
    pub context: bace_gameplay_api::ActionContext,
    pub request: bace_gameplay_api::corpse_consent::CorpseConsentRequest,
    /// Supplied by the runtime adapter; the simulation never reads wall time.
    pub unix_seconds: u64,
}
impl CorpseConsentCommand {
    pub fn valid_bounds(&self) -> bool {
        let name = match &self.request {
            bace_gameplay_api::corpse_consent::CorpseConsentRequest::Clear
            | bace_gameplay_api::corpse_consent::CorpseConsentRequest::Display => None,
            bace_gameplay_api::corpse_consent::CorpseConsentRequest::RemoveFrom(name)
            | bace_gameplay_api::corpse_consent::CorpseConsentRequest::Add(name)
            | bace_gameplay_api::corpse_consent::CorpseConsentRequest::Remove(name) => Some(name),
        };
        self.correlation != 0
            && self.context.actor.0 != 0
            && self.unix_seconds != 0
            && name.is_none_or(|name| !name.is_empty() && name.len() <= 100 && !name.contains('\0'))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseConsentError {
    Ownership,
    Capacity,
    Invalid,
}
#[derive(Clone, Debug)]
pub struct CorpseConsentOutcome {
    pub correlation: u64,
    pub actor: EntityId,
    /// Text is frozen at the authoritative mutation, including the target name.
    pub result: Result<Vec<(EntityId, String)>, CorpseConsentError>,
}

/// Durable corpse rights are supplied by the death checkpoint or cold restore.
/// The active viewer remains transient and belongs to the simulation thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpseAccessProfile {
    pub victim: Option<EntityId>,
    pub killer: Option<EntityId>,
    pub is_monster: bool,
    pub generated_rare: bool,
    pub pk_death: bool,
    pub looted: bool,
    /// One-shot permittees who already opened this corpse may reopen it.
    pub permittees: Vec<EntityId>,
}

impl CorpseAccessProfile {
    pub fn validate(&self) -> Result<(), CorpseAccessError> {
        if self.victim.is_some_and(|id| id.0 == 0)
            || self.killer.is_some_and(|id| id.0 == 0)
            || self.permittees.len() > 1024
            || self.permittees.iter().any(|id| id.0 == 0)
            || self.permittees.windows(2).any(|pair| pair[0] >= pair[1])
            || !self.is_monster && self.victim.is_none()
        {
            return Err(CorpseAccessError::Invalid);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseAccessError {
    Invalid,
    Missing,
    Stale,
    Capacity,
    Ownership,
    OutOfRange,
    Obstructed,
}

#[derive(Clone, Debug)]
pub enum CorpseAccessCommand {
    Inspect {
        correlation: u64,
        context: bace_gameplay_api::ActionContext,
        corpse: EntityId,
        unix_seconds: u64,
    },
    Adopt {
        correlation: u64,
        context: bace_gameplay_api::ActionContext,
        corpse: EntityId,
        has_loot_permit: bool,
        decision: CorpseAccessDecision,
    },
}
impl CorpseAccessCommand {
    pub fn valid_bounds(&self) -> bool {
        match self {
            Self::Inspect {
                correlation,
                context,
                corpse,
                unix_seconds,
            } => *correlation != 0 && context.actor.0 != 0 && corpse.0 != 0 && *unix_seconds != 0,
            Self::Adopt {
                correlation,
                context,
                corpse,
                ..
            } => *correlation != 0 && context.actor.0 != 0 && corpse.0 != 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpseAccessInspection {
    pub operation: u64,
    pub profile: CorpseAccessProfile,
    pub decision: CorpseAccessDecision,
    /// Owner-checked grant at Inspect. Retained for the in-flight durable Open.
    pub has_loot_permit: bool,
}
#[derive(Clone, Debug)]
pub enum CorpseAccessOutcome {
    Inspected {
        correlation: u64,
        actor: EntityId,
        corpse: EntityId,
        result: Result<CorpseAccessInspection, CorpseAccessError>,
    },
    Adopted {
        correlation: u64,
        actor: EntityId,
        corpse: EntityId,
        result: Result<CorpseAccessDecision, CorpseAccessError>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseAccessDenial {
    InUse,
    Rare,
    PlayerKiller,
    Locked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseAccessDecision {
    Open { consume_permit: bool },
    Close { mark_looted: bool },
    Denied(CorpseAccessDenial),
}

#[derive(Clone, Debug)]
pub(crate) struct CorpseAccessState {
    pub(crate) operation: u64,
    pub(crate) profile: CorpseAccessProfile,
    pub(crate) viewer: Option<EntityId>,
}
pub(crate) struct PendingDeath {
    pub(crate) motion: Option<(bace_motion::MotionToken, u16)>,
    pub(crate) start_view: Result<
        bace_gameplay_api::visibility::AcceptedObjectView,
        bace_gameplay_api::visibility::ObjectViewRejection,
    >,
    pub(crate) operation: u64,
    pub(crate) killer: Option<EntityId>,
    pub(crate) last_damager: Option<EntityId>,
    pub(crate) submitted: bool,
    pub(crate) ticket: Option<PlayerDeathTicket>,
    pub(crate) corpse: Option<bace_entity::Actor>,
    pub(crate) world_roots: Vec<bace_entity::Actor>,
    pub(crate) committed: bool,
    pub(crate) due: Option<u64>,
    pub(crate) respawn_due: Option<u64>,
    pub(crate) blocked: Option<PlayerDeathError>,
    pub(crate) corpse_expiry_tick: Option<u64>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PlayerDeathEvent {
    Prepare {
        operation: u64,
        actor: EntityId,
        before_revision: u64,
        killer: Option<EntityId>,
        killer_is_olthoi: bool,
        corpse_killer: Option<(EntityId, String)>,
        olthoi: Option<OlthoiDeathKind>,
        announcement: Option<PlayerDeathAnnouncement>,
    },
    Started {
        operation: u64,
        actor: EntityId,
        motion: u32,
        until_tick: u64,
        num_deaths: u32,
        death_level: Option<u32>,
        vitae_pool: Option<i32>,
        vitae: Option<bace_magic::EnchantmentEntry>,
        purge_bad: bool,
        suicide: bool,
        accepted: Result<
            bace_gameplay_api::visibility::AcceptedObjectView,
            bace_gameplay_api::visibility::ObjectViewRejection,
        >,
    },
    Corpse {
        operation: u64,
        actor: EntityId,
        corpse: EntityId,
        accepted_corpse: Result<
            bace_gameplay_api::visibility::AcceptedObjectView,
            bace_gameplay_api::visibility::ObjectViewRejection,
        >,
        accepted_player: Result<
            bace_gameplay_api::visibility::AcceptedObjectView,
            bace_gameplay_api::visibility::ObjectViewRejection,
        >,
    },
    WorldDrops {
        operation: u64,
        actor: EntityId,
        roots: Vec<(
            EntityId,
            Result<
                bace_gameplay_api::visibility::AcceptedObjectView,
                bace_gameplay_api::visibility::ObjectViewRejection,
            >,
        )>,
        accepted_player: Result<
            bace_gameplay_api::visibility::AcceptedObjectView,
            bace_gameplay_api::visibility::ObjectViewRejection,
        >,
    },
    Respawned {
        operation: u64,
        actor: EntityId,
        destination: bace_interactions::PortalPosition,
        accepted: Result<
            bace_gameplay_api::visibility::AcceptedObjectView,
            bace_gameplay_api::visibility::ObjectViewRejection,
        >,
        vitals: [u32; 3],
        vital_revision: u64,
    },
    ProtectionExpired {
        actor: EntityId,
        recipient: Option<bace_gameplay_api::CharacterBinding>,
    },
    ProtectionDispelled {
        actor: EntityId,
        recipient: Option<bace_gameplay_api::CharacterBinding>,
    },
    PkStatus {
        actor: EntityId,
        status: u32,
        recipient: Option<bace_gameplay_api::CharacterBinding>,
    },
}

/// Text selected from the accepted final blow, before the victim leaves the
/// combat owner. The last damager may differ from the top corpse contributor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerDeathAnnouncement {
    pub last_damager: Option<EntityId>,
    pub victim_text: String,
    pub killer_text: Option<String>,
    pub broadcast_text: String,
}
impl PlayerDeaths {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            states: BTreeMap::new(),
            pending: BTreeMap::new(),
            events: VecDeque::with_capacity(capacity),
            outcomes: VecDeque::with_capacity(1),
            capacity,
            next: 0,
            random: None,
            epoch: 0,
            timers: BTreeMap::new(),
            timer_scratch: Vec::new(),
            corpse_access: BTreeMap::new(),
            corpse_access_outcomes: VecDeque::with_capacity(capacity),
            consent_grants: BTreeMap::new(),
            consent_outcomes: VecDeque::with_capacity(capacity),
        }
    }
    pub(crate) fn owns_inventory(&self, operation: u64) -> bool {
        self.pending.values().any(|p| {
            p.ticket
                .as_ref()
                .is_some_and(|t| t.inventory.operation == operation)
        })
    }
    pub(crate) fn reserved(&self, actor: EntityId) -> bool {
        self.pending.contains_key(&actor)
    }
    pub(crate) fn has_state(&self) -> bool {
        !self.states.is_empty()
            || !self.pending.is_empty()
            || !self.events.is_empty()
            || !self.outcomes.is_empty()
            || !self.corpse_access_outcomes.is_empty()
            || !self.corpse_access.is_empty()
            || !self.consent_grants.is_empty()
            || !self.consent_outcomes.is_empty()
    }
}
/// Cold-prepared inputs, produced from accepted templates and DAT animation.
/// `fresh_stacks` pairs an existing source stack with a new template instance.
pub struct PreparedPlayerDeath {
    pub operation: u64,
    pub actor: EntityId,
    pub corpse: bace_entity::Actor,
    pub corpse_item: bace_inventory::InventoryItem,
    pub corpse_container: bace_inventory::InventoryContainer,
    pub possessions: Vec<bace_interactions::DeathPossession>,
    pub fresh_stacks: Vec<(EntityId, bace_inventory::InventoryItem)>,
    pub coin_stacks: Vec<bace_inventory::InventoryItem>,
    pub animation_seconds: f64,
    /// Immutable DAT secondary-attribute formulas. Base attributes/ranks and all
    /// enchantment modifiers are read from their current simulation owners.
    pub vital_formulas: [bace_character::VitalFormula; 3],
    /// Cold source GearMaxHealth (Int379), exactly once for every wielded item.
    pub equipped_health: Vec<(EntityId, u32)>,
    pub instantiation: Option<bace_interactions::PortalPosition>,
    pub olthoi: Option<PreparedOlthoiDeath>,
}
/// Bool29 follows Creature_Death.CreateCorpse's early world-drop branch. These
/// exact roots never become a corpse or enter corpse access/expiry ownership.
pub struct PreparedPlayerNoCorpse {
    pub operation: u64,
    pub actor: EntityId,
    pub existing: Vec<EntityId>,
    pub fresh_items: Vec<bace_inventory::InventoryItem>,
    pub fresh_containers: Vec<bace_inventory::InventoryContainer>,
    pub world_roots: Vec<bace_entity::Actor>,
    pub accepted_position: bace_content::Position,
    pub animation_seconds: f64,
    pub vital_formulas: [bace_character::VitalFormula; 3],
    pub equipped_health: Vec<(EntityId, u32)>,
    pub instantiation: Option<bace_interactions::PortalPosition>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerNoCorpsePlan {
    pub world_roots: Vec<EntityId>,
    /// Existing contained rows retained beneath a selected direct-pack world
    /// root. They keep their placement but receive an exact revision touch in
    /// the same durable death receipt so the online save owner can adopt it.
    pub descendants: Vec<NoCorpseDescendant>,
    pub accepted_position: bace_content::Position,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoCorpseDescendant {
    pub id: EntityId,
    pub parent: EntityId,
    pub slot: u32,
    pub pack_slot: bool,
    /// Post-receipt gameplay revision, exactly one above the accepted source.
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OlthoiDeathKind {
    Slag,
    Treasure,
    Empty,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedOlthoiDeath {
    pub kind: OlthoiDeathKind,
    pub had_vitae: bool,
    pub before_timestamp: Option<i32>,
    pub after_timestamp: Option<i32>,
    pub items: Vec<bace_inventory::InventoryItem>,
    /// Cold-prepared descendant containers, admitted with the same inventory proposal.
    pub containers: Vec<bace_inventory::InventoryContainer>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerDeathTicket {
    pub operation: u64,
    pub actor: EntityId,
    pub killer: Option<EntityId>,
    pub kind: bace_interactions::PlayerDeathKind,
    pub olthoi: Option<OlthoiDeathKind>,
    pub before_revision: u64,
    pub after_revision: u64,
    pub before: PlayerDeathState,
    pub after: PlayerDeathState,
    /// Frozen ACE death purge branch (PropertyInt 232 and PK death).
    pub purge_bad: bool,
    pub inventory: crate::InventoryTicket,
    /// Exact ACE CreateCorpse selection order and action origins, frozen before
    /// the durable checkpoint. Olthoi death uses its separate authored branch.
    pub inventory_transcript: Option<DeathInventoryTranscript>,
    pub corpse: EntityId,
    /// Distinct Bool29 world-drop transaction. `corpse` is the absent sentinel
    /// only for this branch; no corpse item/snapshot may be constructed.
    pub no_corpse: Option<PlayerNoCorpsePlan>,
    pub corpse_items: Vec<EntityId>,
    pub corpse_decay_seconds: u64,
    pub registry_before_revision: u64,
    pub registry_after_revision: u64,
    pub before_enchantments: Vec<bace_magic::EnchantmentEntry>,
    pub enchantments: Vec<bace_magic::EnchantmentEntry>,
    pub destination: bace_world::WorldTeleport,
    pub vitals: Vec<bace_entity::VitalMutation>,
    pub post_death_maxima: [u32; 3],
    pub animation_ticks: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeathDropOrigin {
    Split,
    Pack,
    Wield,
    SlipperyPack,
    SlipperyWield,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathDropReceipt {
    pub source: EntityId,
    pub dropped: EntityId,
    pub amount: u32,
    pub origin: DeathDropOrigin,
    /// Accepted wielded-location mask before death.
    pub wielded_location: u32,
    pub burden_after: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathDestroyedReceipt {
    pub item: EntityId,
    pub burden_after: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathCoinSource {
    pub source: EntityId,
    pub before: u32,
    pub after: u32,
    pub whole: bool,
    pub burden_after: u64,
    pub coin_value_after: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeathInventoryTranscript {
    pub drops: Vec<DeathDropReceipt>,
    pub destroyed: Vec<DeathDestroyedReceipt>,
    pub coin_sources: Vec<DeathCoinSource>,
    pub coin_drops: Vec<EntityId>,
    pub coin_amount: u32,
    pub pyreals_destroyed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerDeathReceipt {
    pub operation: u64,
    pub actor: EntityId,
    pub after_revision: u64,
    pub inventory: crate::InventoryReceipt,
}
pub enum PlayerDeathCommand {
    Prepare(Box<PreparedPlayerDeath>),
    PrepareNoCorpse(Box<PreparedPlayerNoCorpse>),
    Committed {
        receipt: PlayerDeathReceipt,
        corpse_expiry_tick: u64,
    },
    RegisterCorpseExpiry {
        corpse: EntityId,
        death_operation: u64,
        expires_tick: u64,
    },
    ConfirmCorpseExpiry(crate::InventoryReceipt),
    PrepareCorpseSpill(crate::PreparedCorpseSpill),
    RetryCorpseExpiry {
        operation: u64,
    },
    Retry {
        operation: u64,
    },
}

impl PlayerDeathCommand {
    pub(crate) fn valid_bounds(&self) -> bool {
        match self {
            Self::Prepare(p) => {
                p.possessions.len() <= 1024
                    && p.fresh_stacks.len() <= 1024
                    && p.coin_stacks.len() <= 1024
                    && p.equipped_health.len() <= 1024
                    && p.olthoi.as_ref().is_none_or(|loot| {
                        loot.items.len() <= 120 && loot.containers.len() <= loot.items.len()
                    })
            }
            Self::PrepareNoCorpse(p) => {
                p.operation != 0
                    && p.existing.len() <= 1024
                    && p.fresh_items.len() <= 1024
                    && p.fresh_containers.len() <= 1024
                    && p.world_roots.len() <= 1024
                    && p.equipped_health.len() <= 1024
            }
            Self::Committed { receipt, .. } => receipt.inventory.revisions.len() <= 4096,
            Self::ConfirmCorpseExpiry(r) => r.revisions.len() <= 1024,
            Self::PrepareCorpseSpill(p) => {
                p.operation != 0 && !p.actors.is_empty() && p.actors.len() <= 1023
            }
            Self::Retry { .. }
            | Self::RegisterCorpseExpiry { .. }
            | Self::RetryCorpseExpiry { .. } => true,
        }
    }
}
