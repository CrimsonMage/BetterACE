//! Durable housing composition. Retained storage/hook contents are never deleted
//! by owner changes. Player V2 rare and enchantment fields are cloned intact.
use crate::game_inventory::set;
use bace_housing::{HouseGuest, HousePayment, HousingProposal, HousingReason, HousingState};
use bace_persistence::{HouseOwnershipChange, HousingOperation, PlacementOperation, SaveSnapshot};
use bace_storage_codec::{HouseAccessV1, HousePaymentV2, HouseSaveV3, PlayerSaveV6};
use std::collections::BTreeSet;
#[derive(Debug, thiserror::Error)]
pub enum HousingFreezeError {
    #[error("housing snapshot identity/revision mismatch")]
    Identity,
    #[error("V1 rent/payment state requires explicit prepared migration supplement")]
    MissingRent,
    #[error("housing transaction is missing payment snapshots or currency change")]
    MissingPayment,
    #[error("housing numeric width overflow")]
    Overflow,
    #[error(transparent)]
    Save(#[from] bace_storage_codec::SaveCodecError),
}
pub struct FrozenHousingPlayer {
    pub player: PlayerSaveV6,
    pub persisted_version: i64,
}
pub struct HousingFreezeInput<'a> {
    pub proposal: &'a HousingProposal,
    pub house: &'a HouseSaveV3,
    pub house_version: i64,
    pub players: &'a [FrozenHousingPlayer],
    pub inventory: PlacementOperation,
    pub apartment: bool,
    /// The committed inventory change must include a minted/merged change stack.
    pub change_prepared: bool,
}
pub fn prepare_housing_state(
    saved: &HouseSaveV3,
    interval_seconds: u64,
    rent_supplement: Option<&[HousePaymentV2]>,
) -> Result<HousingState, HousingFreezeError> {
    saved.validate()?;
    let rent = if saved.rent_complete {
        &saved.rent
    } else {
        rent_supplement.ok_or(HousingFreezeError::MissingRent)?
    };
    let state = HousingState {
        house: bace_types::EntityId(saved.entity.object_id),
        revision: saved.entity.mutation_revision,
        owner: saved.owner_id.map(bace_types::EntityId),
        allegiance_monarch: saved
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .find(|p| p.id == 26)
            .map(|p| bace_types::EntityId(p.value)),
        generation: saved.access_generation,
        purchased_at: saved.purchased_at,
        period_start: saved.rent_period_start,
        rent_due: saved.rent_due_at,
        interval_seconds,
        maintenance_free: saved.maintenance_free,
        open: saved.open_to_all,
        storage_open: false,
        hooks_visible: saved.hooks_visible,
        guests: saved
            .access
            .iter()
            .map(|a| HouseGuest {
                player: bace_types::EntityId(a.player_id),
                storage: a.permissions & 2 != 0,
            })
            .collect(),
        rent: rent
            .iter()
            .map(|p| HousePayment {
                template: p.template,
                required: p.required,
                paid: p.paid,
            })
            .collect(),
    };
    state.validate().map_err(|_| HousingFreezeError::Identity)?;
    Ok(state)
}
pub fn freeze_housing(
    mut input: HousingFreezeInput<'_>,
) -> Result<HousingOperation, HousingFreezeError> {
    let before = &input.proposal.before;
    let after = &input.proposal.after;
    let saved = input.house;
    if before.house.0 != saved.entity.object_id
        || before.revision != saved.entity.mutation_revision
        || before.owner.map(|v| v.0) != saved.owner_id
        || before.allegiance_monarch.map(|id| id.0)
            != saved
                .entity
                .state
                .properties
                .instance_ids
                .iter()
                .find(|p| p.id == 26)
                .map(|p| p.value)
        || before.generation != saved.access_generation
        || input
            .inventory
            .snapshots
            .iter()
            .any(|s| s.object_id == saved.entity.object_id)
    {
        return Err(HousingFreezeError::Identity);
    }
    if input.proposal.currency_change != 0 && !input.change_prepared {
        return Err(HousingFreezeError::MissingPayment);
    }
    for payment in &input.proposal.payments {
        if !input.inventory.snapshots.iter().any(|s| {
            s.object_id == payment.item.0
                && s.mutation_revision == payment.revision.saturating_add(1)
        }) {
            return Err(HousingFreezeError::MissingPayment);
        }
    }
    let mut house = saved.clone();
    house.entity.mutation_revision = after.revision;
    house.owner_id = after.owner.map(|v| v.0);
    house
        .entity
        .state
        .properties
        .instance_ids
        .retain(|p| p.id != 26);
    if let Some(monarch) = after.allegiance_monarch {
        set(
            &mut house.entity.state.properties.instance_ids,
            26,
            monarch.0,
        );
    }
    house.access_generation = after.generation;
    house.purchased_at = after.purchased_at;
    house.rent_period_start = after.period_start;
    house.rent_due_at = after.rent_due;
    house.open_to_all = after.open;
    house.storage_open = false;
    house.hooks_visible = after.hooks_visible;
    house.maintenance_free = after.maintenance_free;
    house.rent_complete = true;
    house.access = after
        .guests
        .iter()
        .map(|g| HouseAccessV1 {
            player_id: g.player.0,
            permissions: 1 | if g.storage { 2 } else { 0 },
        })
        .collect();
    house.rent = after
        .rent
        .iter()
        .map(|p| HousePaymentV2 {
            template: p.template,
            required: p.required,
            paid: p.paid,
        })
        .collect();
    house
        .entity
        .state
        .properties
        .instance_ids
        .retain(|p| p.id != 32);
    house.entity.state.properties.strings.retain(|p| p.id != 36);
    house.entity.state.properties.ints.retain(|p| p.id != 141);
    if after.open {
        set(&mut house.entity.state.properties.ints, 141, 1)
    }
    set(
        &mut house.entity.state.properties.bools,
        75,
        after.hooks_visible,
    );
    if let Some(owner) = after.owner {
        set(&mut house.entity.state.properties.instance_ids, 32, owner.0);
        let player = input
            .players
            .iter()
            .find(|p| p.player.player.entity.object_id == owner.0)
            .ok_or(HousingFreezeError::Identity)?;
        set(
            &mut house.entity.state.properties.strings,
            36,
            player.player.player.name.clone(),
        );
    }
    let owners: BTreeSet<_> = [before.owner, after.owner].into_iter().flatten().collect();
    for owner in owners {
        let source = input
            .players
            .iter()
            .find(|p| p.player.player.entity.object_id == owner.0)
            .ok_or(HousingFreezeError::Identity)?;
        let existing = input
            .inventory
            .snapshots
            .iter()
            .position(|s| s.object_id == owner.0);
        let mut player = if let Some(index) = existing {
            let snapshot = &input.inventory.snapshots[index];
            if snapshot.expected_version != source.persisted_version {
                return Err(HousingFreezeError::Identity);
            }
            PlayerSaveV6::decode_or_migrate(&snapshot.bytes)?
        } else {
            let mut p = source.player.clone();
            p.player.entity.mutation_revision = p
                .player
                .entity
                .mutation_revision
                .checked_add(1)
                .ok_or(HousingFreezeError::Overflow)?;
            p
        };
        let properties = &mut player.player.entity.state.properties;
        if after.owner == Some(owner) {
            set(&mut properties.data_ids, 42, house.house_id);
            set(&mut properties.instance_ids, 33, house.entity.object_id);
            set(
                &mut properties.ints,
                9011,
                i32::try_from(after.rent_due).map_err(|_| HousingFreezeError::Overflow)?,
            );
            if input.proposal.reason == HousingReason::Purchase && !input.apartment {
                set(
                    &mut properties.ints,
                    199,
                    i32::try_from(after.purchased_at).map_err(|_| HousingFreezeError::Overflow)?,
                );
            }
        } else if properties
            .instance_ids
            .iter()
            .find(|p| p.id == 33)
            .is_none_or(|p| p.value == house.entity.object_id)
        {
            properties.data_ids.retain(|p| p.id != 42);
            properties.instance_ids.retain(|p| p.id != 33);
            properties.ints.retain(|p| p.id != 9011);
            if input.proposal.reason == HousingReason::Eviction {
                set(&mut properties.bools, 9003, true);
            }
        }
        let snapshot = SaveSnapshot {
            object_id: owner.0,
            mutation_revision: player.player.entity.mutation_revision,
            expected_version: source.persisted_version,
            bytes: player.encode()?,
        };
        if let Some(index) = existing {
            input.inventory.snapshots[index] = snapshot
        } else {
            input.inventory.snapshots.push(snapshot);
        }
    }
    input.inventory.snapshots.push(SaveSnapshot {
        object_id: house.entity.object_id,
        mutation_revision: house.entity.mutation_revision,
        expected_version: input.house_version,
        bytes: house.encode()?,
    });
    let mut participants: BTreeSet<_> = input.inventory.participants.into_iter().collect();
    participants.extend(input.inventory.snapshots.iter().map(|s| s.object_id));
    input.inventory.participants = participants.into_iter().collect();
    Ok(HousingOperation {
        inventory: input.inventory,
        ownership: HouseOwnershipChange {
            house: house.entity.object_id,
            house_id: house.house_id,
            expected_owner: before.owner.map(|v| v.0),
            expected_generation: before.generation,
            owner: after.owner.map(|v| v.0),
            generation: after.generation,
        },
    })
}
/// Offline preparation does not load a physics actor or clear an owner's dirty
/// state. The exact operation stays caller-owned through queue/commit uncertainty.
pub struct OfflineRentInput {
    pub house: HouseSaveV3,
    pub house_version: i64,
    pub player: PlayerSaveV6,
    pub player_version: i64,
    pub lease: bace_persistence::CharacterLease,
    pub interval_seconds: u64,
    pub now: i64,
    pub rent_enabled: bool,
    pub requirements_met: bool,
    pub apartment: bool,
    pub rent_supplement: Option<Vec<HousePaymentV2>>,
}
pub fn prepare_offline_rent(
    input: OfflineRentInput,
) -> Result<Option<HousingOperation>, HousingFreezeError> {
    if input.lease.state != bace_persistence::OwnershipState::Offline
        || input.house.owner_id != Some(input.lease.character_id)
        || input.player.player.entity.object_id != input.lease.character_id
    {
        return Err(HousingFreezeError::Identity);
    }
    let state = prepare_housing_state(
        &input.house,
        input.interval_seconds,
        input.rent_supplement.as_deref(),
    )?;
    let Some(proposal) = bace_housing::propose_due_rent(
        &state,
        input.now,
        input.rent_enabled,
        input.requirements_met,
    )
    .map_err(|_| HousingFreezeError::Identity)?
    else {
        return Ok(None);
    };
    let operation_id = format!(
        "rent:{}:{}:{}",
        state.house.0, state.generation, state.rent_due
    );
    let players = [FrozenHousingPlayer {
        player: input.player,
        persisted_version: input.player_version,
    }];
    let inventory = PlacementOperation {
        operation_id,
        snapshots: vec![],
        participants: vec![state.house.0, input.lease.character_id],
        leases: vec![input.lease],
        changes: vec![],
        storage_views: vec![],
    };
    freeze_housing(HousingFreezeInput {
        proposal: &proposal,
        house: &input.house,
        house_version: input.house_version,
        players: &players,
        inventory,
        apartment: input.apartment,
        change_prepared: false,
    })
    .map(Some)
}
