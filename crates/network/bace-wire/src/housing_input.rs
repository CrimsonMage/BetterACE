//! Pinned ACE housing GameAction readers for the 19 opcodes without a decoder elsewhere.
//! Official ACE `GameAction/Actions/GameActionHouse*.cs` at 47edade3bd3f6044b676d4eb877c4965c7eda62b.
//! Decode only: runtime ingress is not wired here. Motd (0x0254-0x0256) and
//! DoAllegianceHouseAction (0x0042) are decoded by `group_input`.
use crate::{Reader, WireError, opcode::GameActionType as Op};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HousingAction {
    BuyHouse {
        slumlord: u32,
        items: Vec<u32>,
    },
    HouseQuery,
    AbandonHouse,
    RentHouse {
        slumlord: u32,
        items: Vec<u32>,
    },
    AddPermanentGuest(String),
    RemovePermanentGuest(String),
    SetOpenHouseStatus(bool),
    ChangeStoragePermission {
        name: String,
        enabled: bool,
    },
    BootSpecificHouseGuest(String),
    RemoveAllStoragePermission,
    RequestFullGuestList,
    QueryLord(u32),
    AddAllStoragePermission,
    RemoveAllPermanentGuests,
    BootEveryone,
    SetHooksVisibility(bool),
    ModifyAllegianceGuestPermission(bool),
    ModifyAllegianceStoragePermission(bool),
    /// Raw `HouseType` value; mapping to a house type is left to the owner.
    ListAvailableHouses(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HousingRequest {
    pub action: HousingAction,
    pub trailing_bytes: usize,
}

impl HousingRequest {
    /// `max_items` bounds the `ReadListUInt32` count of BuyHouse and RentHouse.
    pub fn decode(
        op: Op,
        payload: &[u8],
        max_payload: usize,
        max_string: usize,
        max_items: usize,
    ) -> Result<Self, WireError> {
        if payload.len() > max_payload {
            return Err(WireError::LimitExceeded);
        }
        let mut r = Reader::new(payload);
        let action = match op {
            Op::BuyHouse => {
                let slumlord = r.u32()?;
                let items = item_list(&mut r, max_items)?;
                HousingAction::BuyHouse { slumlord, items }
            }
            Op::HouseQuery => HousingAction::HouseQuery,
            Op::AbandonHouse => HousingAction::AbandonHouse,
            Op::RentHouse => {
                let slumlord = r.u32()?;
                let items = item_list(&mut r, max_items)?;
                HousingAction::RentHouse { slumlord, items }
            }
            Op::AddPermanentGuest => {
                HousingAction::AddPermanentGuest(r.client_string16(max_string)?)
            }
            Op::RemovePermanentGuest => {
                HousingAction::RemovePermanentGuest(r.client_string16(max_string)?)
            }
            Op::SetOpenHouseStatus => HousingAction::SetOpenHouseStatus(r.u32()? != 0),
            Op::ChangeStoragePermission => HousingAction::ChangeStoragePermission {
                name: r.client_string16(max_string)?,
                enabled: r.u32()? != 0,
            },
            Op::BootSpecificHouseGuest => {
                HousingAction::BootSpecificHouseGuest(r.client_string16(max_string)?)
            }
            Op::RemoveAllStoragePermission => HousingAction::RemoveAllStoragePermission,
            Op::RequestFullGuestList => HousingAction::RequestFullGuestList,
            Op::QueryLord => HousingAction::QueryLord(r.u32()?),
            Op::AddAllStoragePermission => HousingAction::AddAllStoragePermission,
            Op::RemoveAllPermanentGuests => HousingAction::RemoveAllPermanentGuests,
            Op::BootEveryone => HousingAction::BootEveryone,
            Op::SetHooksVisibility => HousingAction::SetHooksVisibility(r.u32()? != 0),
            Op::ModifyAllegianceGuestPermission => {
                HousingAction::ModifyAllegianceGuestPermission(r.u32()? != 0)
            }
            Op::ModifyAllegianceStoragePermission => {
                HousingAction::ModifyAllegianceStoragePermission(r.u32()? != 0)
            }
            Op::ListAvailableHouses => HousingAction::ListAvailableHouses(r.u32()?),
            _ => return Err(WireError::UnexpectedOpcode(op.0)),
        };
        Ok(Self {
            action,
            trailing_bytes: r.remaining(),
        })
    }
}

/// ACE `PackableList.ReadListUInt32`: u32 count, then `count` u32 items.
/// The count is untrusted: it is bounded before any allocation and must fit the remaining bytes.
fn item_list(r: &mut Reader<'_>, max_items: usize) -> Result<Vec<u32>, WireError> {
    let count = usize::try_from(r.u32()?).map_err(|_| WireError::LimitExceeded)?;
    if count > max_items {
        return Err(WireError::LimitExceeded);
    }
    let needed = count.checked_mul(4).ok_or(WireError::InvalidLength)?;
    if needed > r.remaining() {
        return Err(WireError::Truncated);
    }
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(r.u32()?);
    }
    Ok(items)
}
