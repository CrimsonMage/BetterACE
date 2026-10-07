//! Pinned ApproachVendor payload. Prices, visible stock and currency totals are
//! explicit projections; this encoder cannot authorize or commit a purchase.
use crate::envelope::message_writer;
use crate::opcode::{GameEventType, GameMessageOpcode};
use crate::{ObjectCodecLimits, ObjectGameData, WireError};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorCurrency {
    pub class_id: u32,
    pub amount: u32,
    pub plural_name: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct VendorListingItem {
    /// -1 denotes unlimited stock. Other values must fit the low 24 bits.
    pub quantity: i32,
    pub object_id: u32,
    pub game: ObjectGameData,
}
#[derive(Clone, Debug, PartialEq)]
pub struct VendorListing {
    pub vendor_id: u32,
    pub merchandise_types: u32,
    pub minimum_value: u32,
    pub maximum_value: u32,
    pub deal_magical: bool,
    pub buy_price: f32,
    pub sell_price: f32,
    pub alternate_currency: Option<VendorCurrency>,
    /// Ordered default stock followed by unique stock, matching ACE's traversal.
    pub items: Vec<VendorListingItem>,
}
impl VendorListing {
    pub fn encode(
        &self,
        player_id: u32,
        sequence: u32,
        max_items: usize,
        limits: ObjectCodecLimits,
    ) -> Result<Vec<u8>, WireError> {
        if self.items.len() > max_items || self.items.len() > i32::MAX as usize {
            return Err(WireError::LimitExceeded);
        }
        if self
            .items
            .iter()
            .any(|item| !(-1..=0x00ffffff).contains(&item.quantity))
        {
            return Err(WireError::InvalidEncoding);
        }
        let mut writer = message_writer(GameMessageOpcode::GameEvent);
        for value in [
            player_id,
            sequence,
            GameEventType::ApproachVendor.0,
            self.vendor_id,
            self.merchandise_types,
            self.minimum_value,
            self.maximum_value,
            u32::from(self.deal_magical),
        ] {
            writer.u32(value);
        }
        writer.f32(self.buy_price);
        writer.f32(self.sell_price);
        if let Some(currency) = &self.alternate_currency {
            if currency.plural_name.chars().count() > limits.max_string_bytes {
                return Err(WireError::LimitExceeded);
            }
            writer.u32(currency.class_id);
            writer.u32(currency.amount);
            writer.string16(&currency.plural_name)?;
        } else {
            writer.u32(0);
            writer.u32(0);
            writer.string16("")?;
        }
        writer.u32(self.items.len() as u32);
        for item in &self.items {
            writer.u32((item.quantity as u32 & 0x00ffffff) | 0xff000000);
            writer.u32(item.object_id);
            item.game.write(
                &mut writer,
                limits.max_string_bytes,
                limits.max_restrictions,
            )?;
            if writer.position() > limits.max_message_bytes {
                return Err(WireError::LimitExceeded);
            }
        }
        writer.align4();
        if writer.position() > limits.max_message_bytes {
            return Err(WireError::LimitExceeded);
        }
        Ok(writer.into_bytes())
    }
}
