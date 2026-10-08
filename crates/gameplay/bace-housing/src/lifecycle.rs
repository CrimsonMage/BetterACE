//! ACE HouseManager/Player_House lifecycle as pure durable proposals. Ownership,
//! payment consumption and retained contents must commit in one transaction.
use bace_gameplay_api::HousingRejection as Error;
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HouseGuest {
    pub player: EntityId,
    pub storage: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HousePayment {
    pub template: u32,
    pub required: u32,
    pub paid: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HousingState {
    pub house: EntityId,
    pub revision: u64,
    pub owner: Option<EntityId>,
    /// ACE IID26: allegiance access enabled for this monarch.
    pub allegiance_monarch: Option<EntityId>,
    pub generation: u64,
    pub purchased_at: i64,
    pub period_start: i64,
    pub rent_due: i64,
    pub interval_seconds: u64,
    pub maintenance_free: bool,
    pub open: bool,
    pub storage_open: bool,
    pub hooks_visible: bool,
    pub guests: Vec<HouseGuest>,
    pub rent: Vec<HousePayment>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HousingActor {
    pub actor: EntityId,
    pub account: u64,
    pub level: u32,
    pub monarch: bool,
    pub allegiance_rank: u32,
    pub account_age_seconds: u64,
    pub previous_purchase: i64,
    pub owns_house: bool,
    pub in_range: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurchaseRules {
    pub minimum_level: u32,
    pub requires_monarch: bool,
    pub minimum_rank: u32,
    pub account_age_seconds: u64,
    pub cooldown_seconds: u64,
    pub apartment: bool,
    pub buy: Vec<HousePayment>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaymentItem {
    pub item: EntityId,
    pub template: u32,
    pub revision: u64,
    pub count: u32,
    pub currency_value: u32,
    pub trade_note: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaymentUse {
    pub item: EntityId,
    pub revision: u64,
    pub count: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HousingReason {
    Purchase,
    RentPayment,
    Renewal,
    Abandon,
    Eviction,
    Permissions,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HousingProposal {
    pub reason: HousingReason,
    pub before: HousingState,
    pub after: HousingState,
    pub payments: Vec<PaymentUse>,
    pub currency_change: u64,
    pub ownership_changed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HouseView {
    pub house: EntityId,
    pub generation: u64,
    pub player: EntityId,
    pub storage: bool,
}
impl HousingState {
    pub fn validate(&self) -> Result<(), Error> {
        if self.house.0 == 0
            || self.allegiance_monarch.is_some_and(|id| id.0 == 0)
            || self.generation == 0
            || self.interval_seconds == 0
            || self.interval_seconds > i64::MAX as u64
            || self.purchased_at < 0
            || self.period_start < 0
            || self.rent_due < self.period_start
            || self.guests.len() > 4096
            || self.rent.len() > 256
        {
            return Err(Error::Requirements);
        }
        let mut ids = BTreeSet::new();
        for g in &self.guests {
            if g.player.0 == 0 || !ids.insert(g.player) {
                return Err(Error::Requirements);
            }
        }
        ids.clear();
        for p in &self.rent {
            if p.template == 0
                || p.required == 0
                || p.paid > p.required
                || !ids.insert(EntityId(p.template))
            {
                return Err(Error::InvalidPayment);
            }
        }
        if self.owner.is_none()
            && (!self.guests.is_empty()
                || self.open
                || self.storage_open
                || self.allegiance_monarch.is_some())
        {
            return Err(Error::AccessDenied);
        }
        Ok(())
    }
    pub fn permits(&self, player: EntityId, storage: bool) -> bool {
        self.owner.is_some()
            && (self.owner == Some(player)
                || if storage {
                    self.guests.iter().any(|g| g.player == player && g.storage)
                } else {
                    self.open || self.guests.iter().any(|g| g.player == player)
                })
    }
    pub fn open_view(&self, player: EntityId, storage: bool) -> Result<HouseView, Error> {
        if !self.permits(player, storage) {
            return Err(Error::AccessDenied);
        }
        Ok(HouseView {
            house: self.house,
            generation: self.generation,
            player,
            storage,
        })
    }
    pub fn validate_view(&self, view: HouseView) -> Result<(), Error> {
        if view.house != self.house || view.generation != self.generation {
            return Err(Error::StaleView);
        }
        if !self.permits(view.player, view.storage) {
            return Err(Error::AccessDenied);
        }
        Ok(())
    }
}
fn proposal(state: &HousingState) -> Result<HousingProposal, Error> {
    state.validate()?;
    let mut after = state.clone();
    after.revision = after.revision.checked_add(1).ok_or(Error::Overflow)?;
    Ok(HousingProposal {
        reason: HousingReason::Permissions,
        before: state.clone(),
        after,
        payments: Vec::new(),
        currency_change: 0,
        ownership_changed: false,
    })
}
fn generation(state: &mut HousingState) -> Result<(), Error> {
    state.generation = state
        .generation
        .checked_add(1)
        .filter(|g| *g <= i64::MAX as u64)
        .ok_or(Error::Overflow)?;
    Ok(())
}
/// Item list is already ownership-reserved and identified by the inventory owner.
/// Coins use WCID273; trade notes are indivisible currency values. Change is an
/// explicit mint proposal, not optimistic client credit.
fn pay(
    requirements: &[HousePayment],
    items: &[PaymentItem],
    partial: bool,
) -> Result<(Vec<PaymentUse>, Vec<HousePayment>, u64), Error> {
    if requirements.len() > 256 || items.len() > 1024 {
        return Err(Error::Capacity);
    }
    let mut ids = BTreeSet::new();
    for i in items {
        if i.item.0 == 0 || i.count == 0 || !ids.insert(i.item) {
            return Err(Error::InvalidPayment);
        }
    }
    let mut remaining: BTreeMap<_, _> = items.iter().map(|i| (i.item, i.count)).collect();
    let mut used = BTreeMap::new();
    let mut result = requirements.to_vec();
    let mut change = 0u64;
    for requirement in &mut result {
        if requirement.required == 0 || requirement.paid > requirement.required {
            return Err(Error::InvalidPayment);
        }
        let mut needed = u64::from(requirement.required - requirement.paid);
        for item in items {
            if needed == 0 {
                break;
            }
            if item.template != requirement.template
                && !(requirement.template == 273 && item.trade_note)
            {
                continue;
            }
            let available = *remaining.get(&item.item).ok_or(Error::InvalidPayment)?;
            let denomination = if item.trade_note && requirement.template == 273 {
                u64::from(item.currency_value)
            } else {
                1
            };
            if denomination == 0 {
                return Err(Error::InvalidPayment);
            }
            let count = u64::from(available).min(needed.div_ceil(denomination));
            let value = count.checked_mul(denomination).ok_or(Error::Overflow)?;
            let credited = needed.min(value);
            needed -= credited;
            requirement.paid += credited as u32;
            change = change
                .checked_add(value - credited)
                .ok_or(Error::Overflow)?;
            remaining.insert(item.item, available - count as u32);
            *used.entry(item.item).or_insert(0u32) += count as u32;
        }
        if needed != 0 && !partial {
            return Err(Error::InsufficientPayment);
        }
    }
    let uses = used
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(id, count)| PaymentUse {
            item: id,
            revision: items
                .iter()
                .find(|i| i.item == id)
                .expect("validated payment")
                .revision,
            count,
        })
        .collect();
    Ok((uses, result, change))
}
pub fn propose_purchase(
    state: &HousingState,
    actor: HousingActor,
    rules: &PurchaseRules,
    items: &[PaymentItem],
    now: i64,
) -> Result<HousingProposal, Error> {
    if state.owner.is_some() || actor.owns_house {
        return Err(Error::AlreadyOwned);
    }
    if !actor.in_range {
        return Err(Error::OutOfRange);
    }
    if now < 0
        || actor.actor.0 == 0
        || actor.account == 0
        || actor.level < rules.minimum_level
        || rules.requires_monarch && !actor.monarch
        || actor.allegiance_rank < rules.minimum_rank
    {
        return Err(Error::Requirements);
    }
    if !rules.apartment
        && (actor.account_age_seconds < rules.account_age_seconds
            || actor.previous_purchase > 0
                && u64::try_from(
                    now.checked_sub(actor.previous_purchase)
                        .ok_or(Error::Overflow)?,
                )
                .map_err(|_| Error::Requirements)?
                    < rules.cooldown_seconds)
    {
        return Err(Error::Requirements);
    }
    let (payments, _, currency_change) = pay(&rules.buy, items, false)?;
    let mut p = proposal(state)?;
    p.reason = HousingReason::Purchase;
    p.after.owner = Some(actor.actor);
    p.after.purchased_at = now;
    p.after.period_start = now;
    p.after.rent_due = now
        .checked_add(state.interval_seconds as i64)
        .ok_or(Error::Overflow)?;
    for rent in &mut p.after.rent {
        rent.paid = 0;
    }
    generation(&mut p.after)?;
    p.payments = payments;
    p.currency_change = currency_change;
    p.ownership_changed = true;
    Ok(p)
}
pub fn propose_rent(
    state: &HousingState,
    actor: EntityId,
    items: &[PaymentItem],
) -> Result<HousingProposal, Error> {
    if state.owner != Some(actor) {
        return Err(Error::NotOwner);
    }
    let (payments, rent, currency_change) = pay(&state.rent, items, true)?;
    if payments.is_empty() {
        return Err(Error::InvalidPayment);
    }
    let mut p = proposal(state)?;
    p.reason = HousingReason::RentPayment;
    p.after.rent = rent;
    p.payments = payments;
    p.currency_change = currency_change;
    Ok(p)
}
/// Source payment-due semantics are strict `now > rent_due`. All periods are
/// purchase-aligned; an overdue restart advances directly to the next boundary.
pub fn propose_due_rent(
    state: &HousingState,
    now: i64,
    rent_enabled: bool,
    requirements_met: bool,
) -> Result<Option<HousingProposal>, Error> {
    state.validate()?;
    if state.owner.is_none() || now <= state.rent_due {
        return Ok(None);
    }
    if now < state.purchased_at {
        return Err(Error::Requirements);
    }
    if rent_enabled
        && !state.maintenance_free
        && (!requirements_met || state.rent.iter().any(|p| p.paid < p.required))
    {
        return Ok(Some(propose_eviction(state)?));
    }
    let mut p = proposal(state)?;
    p.reason = HousingReason::Renewal;
    let periods = (now - state.purchased_at) as u64 / state.interval_seconds;
    let offset = periods
        .checked_mul(state.interval_seconds)
        .ok_or(Error::Overflow)?;
    p.after.period_start = state
        .purchased_at
        .checked_add(i64::try_from(offset).map_err(|_| Error::Overflow)?)
        .ok_or(Error::Overflow)?;
    p.after.rent_due = p
        .after
        .period_start
        .checked_add(state.interval_seconds as i64)
        .ok_or(Error::Overflow)?;
    for rent in &mut p.after.rent {
        rent.paid = 0;
    }
    Ok(Some(p))
}
pub fn propose_abandon(state: &HousingState, actor: EntityId) -> Result<HousingProposal, Error> {
    if state.owner != Some(actor) {
        return Err(Error::NotOwner);
    }
    let mut proposal = propose_eviction(state)?;
    proposal.reason = HousingReason::Abandon;
    Ok(proposal)
}
pub fn propose_eviction(state: &HousingState) -> Result<HousingProposal, Error> {
    if state.owner.is_none() {
        return Err(Error::NotOwner);
    }
    let mut p = proposal(state)?;
    p.reason = HousingReason::Eviction;
    p.after.owner = None;
    p.after.allegiance_monarch = None;
    p.after.guests.clear();
    p.after.open = false;
    p.after.storage_open = false;
    p.after.hooks_visible = true;
    for rent in &mut p.after.rent {
        rent.paid = 0;
    }
    generation(&mut p.after)?;
    p.ownership_changed = true;
    Ok(p)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionChange {
    Open(bool),
    StorageOpen(bool),
    HooksVisible(bool),
    Guest { player: EntityId, storage: bool },
    RemoveGuest(EntityId),
    ClearGuests,
    ClearStorage,
}
pub fn propose_permission(
    state: &HousingState,
    actor: EntityId,
    change: PermissionChange,
) -> Result<HousingProposal, Error> {
    if state.owner != Some(actor) {
        return Err(Error::NotOwner);
    }
    let mut p = proposal(state)?;
    match change {
        PermissionChange::Open(value) => p.after.open = value,
        PermissionChange::StorageOpen(value) => {
            p.after.storage_open = false;
            for guest in &mut p.after.guests {
                guest.storage = value;
            }
        }
        PermissionChange::HooksVisible(value) => p.after.hooks_visible = value,
        PermissionChange::Guest { player, storage } => {
            if player.0 == 0 {
                return Err(Error::Requirements);
            }
            if let Some(g) = p.after.guests.iter_mut().find(|g| g.player == player) {
                g.storage = storage
            } else {
                if p.after.guests.len() == 4096 {
                    return Err(Error::Capacity);
                }
                p.after.guests.push(HouseGuest { player, storage });
                p.after.guests.sort_by_key(|g| g.player);
            }
        }
        PermissionChange::RemoveGuest(id) => p.after.guests.retain(|g| g.player != id),
        PermissionChange::ClearGuests => p.after.guests.clear(),
        PermissionChange::ClearStorage => {
            p.after.storage_open = false;
            for g in &mut p.after.guests {
                g.storage = false;
            }
        }
    }
    generation(&mut p.after)?;
    Ok(p)
}
/// Bounded due queue. Failed work keeps its original deadline until acknowledged.
pub struct RentScheduler {
    entries: BTreeSet<(i64, EntityId)>,
    capacity: usize,
}
impl RentScheduler {
    pub fn new(capacity: usize) -> Result<Self, Error> {
        if capacity == 0 || capacity > 65536 {
            return Err(Error::Capacity);
        }
        Ok(Self {
            entries: BTreeSet::new(),
            capacity,
        })
    }
    pub fn schedule(&mut self, house: EntityId, due: i64) -> Result<(), Error> {
        if due < 0 || house.0 == 0 {
            return Err(Error::Requirements);
        }
        let old = self.entries.iter().find(|(_, id)| *id == house).copied();
        if old.is_none() && self.entries.len() == self.capacity {
            return Err(Error::Capacity);
        }
        if let Some(old) = old {
            if old.0 == due {
                return Ok(());
            }
            return Err(Error::DurabilityPending);
        }
        self.entries.insert((due, house));
        Ok(())
    }
    pub fn next_due(&self, now: i64) -> Option<(i64, EntityId)> {
        self.entries.first().copied().filter(|(due, _)| now > *due)
    }
    pub fn acknowledge(&mut self, ticket: (i64, EntityId)) -> bool {
        self.entries.remove(&ticket)
    }
}

/// Pinned House.GetRentTimestamp/GetRentDue with explicit time and checked overflow.
pub fn rent_window(
    purchase: i64,
    now: i64,
    base_interval: u64,
    apartment: bool,
) -> Result<(i64, i64), Error> {
    if purchase < 0 || now < purchase || base_interval == 0 {
        return Err(Error::Requirements);
    }
    let interval = base_interval
        .checked_mul(if apartment { 3 } else { 1 })
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or(Error::Overflow)?;
    let periods = (now - purchase) as u64 / interval;
    let offset = periods.checked_mul(interval).ok_or(Error::Overflow)?;
    let start = purchase
        .checked_add(i64::try_from(offset).map_err(|_| Error::Overflow)?)
        .ok_or(Error::Overflow)?;
    Ok((
        start,
        start.checked_add(interval as i64).ok_or(Error::Overflow)?,
    ))
}
impl RentScheduler {
    /// Validate both ends before replacing a deadline; use only on committed state.
    pub fn validate_replace(
        &self,
        house: EntityId,
        expected: Option<i64>,
        next: Option<i64>,
    ) -> Result<(), Error> {
        let current = self.entries.iter().find(|(_, id)| *id == house).copied();
        if current.map(|(due, _)| due) != expected {
            return Err(Error::StaleView);
        }
        if house.0 == 0 || next.is_some_and(|v| v < 0) {
            return Err(Error::Requirements);
        }
        if current.is_none() && next.is_some() && self.entries.len() == self.capacity {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    pub fn replace(
        &mut self,
        house: EntityId,
        expected: Option<i64>,
        next: Option<i64>,
    ) -> Result<(), Error> {
        let current = self.entries.iter().find(|(_, id)| *id == house).copied();
        if current.map(|(due, _)| due) != expected {
            return Err(Error::StaleView);
        }
        if house.0 == 0 || next.is_some_and(|v| v < 0) {
            return Err(Error::Requirements);
        }
        if current.is_none() && next.is_some() && self.entries.len() == self.capacity {
            return Err(Error::Capacity);
        }
        if let Some(ticket) = current {
            self.entries.remove(&ticket);
        }
        if let Some(due) = next {
            self.entries.insert((due, house));
        }
        Ok(())
    }
}
