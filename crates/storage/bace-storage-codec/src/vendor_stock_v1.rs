//! Frozen, unplaced vendor stock owner. The ordered entries refer to separately
//! saved ItemSaveV5 trees; this record is never restored as a world item.
use crate::{CodecLimits, SaveCodecError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const VENDOR_STOCK_KIND: u16 = 105;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VendorDefaultStockV1 {
    pub root: u32,
    /// ACE vendor listing count; -1 means unlimited.
    pub display_quantity: i32,
    /// Units retained from the source generator contribution. Zero identifies
    /// authored lazy Shop stock, which has no generator contribution.
    pub contribution_units: u32,
    /// Complete descendant identities in source construction order.
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub child_ids: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VendorUniqueStockV1 {
    pub root: u32,
    pub display_quantity: i32,
    /// Trusted sale time for source unique-stock expiry; absent for a retained
    /// unique item whose sale provenance predates this schema.
    pub sold_at_seconds: Option<i64>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub child_ids: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VendorStockSaveV1 {
    pub marker_object_id: u32,
    pub vendor_object_id: u32,
    pub source_revision: u64,
    pub source_hash: [u8; 32],
    pub loaded: bool,
    pub stock_revision: u64,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub defaults: Vec<VendorDefaultStockV1>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub unique: Vec<VendorUniqueStockV1>,
}

impl VendorStockSaveV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        if self.marker_object_id < 0x8000_0000
            || self.marker_object_id == u32::MAX
            || self.vendor_object_id == 0
            || self.vendor_object_id == self.marker_object_id
            || self.source_revision == 0
            || self.source_hash == [0; 32]
            || !self.loaded
            || self.stock_revision == 0
            || self.defaults.len() + self.unique.len() > 4096
        {
            return Err(SaveCodecError::Invalid("vendor stock identity or bounds"));
        }
        let mut ids = BTreeSet::new();
        let mut total = 0_usize;
        for (root, quantity, children) in self
            .defaults
            .iter()
            .map(|entry| (entry.root, entry.display_quantity, &entry.child_ids))
            .chain(
                self.unique
                    .iter()
                    .map(|entry| (entry.root, entry.display_quantity, &entry.child_ids)),
            )
        {
            total = total
                .checked_add(children.len() + 1)
                .ok_or(SaveCodecError::Invalid("vendor stock tree bounds"))?;
            if total > 8192
                || children.len() > 1023
                || !(-1..=0x00ff_ffff).contains(&quantity)
                || root == 0
                || root == self.vendor_object_id
                || root == self.marker_object_id
                || !ids.insert(root)
                || children.iter().any(|id| {
                    *id == 0
                        || *id == self.vendor_object_id
                        || *id == self.marker_object_id
                        || !ids.insert(*id)
                })
            {
                return Err(SaveCodecError::Invalid("vendor stock ordered trees"));
            }
        }
        if self
            .unique
            .iter()
            .any(|entry| entry.sold_at_seconds.is_some_and(|time| time < 0))
        {
            return Err(SaveCodecError::Invalid("vendor unique sale time"));
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(VENDOR_STOCK_KIND, 1, self, limits())?)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, VENDOR_STOCK_KIND, 1, limits())?;
        value.validate()?;
        Ok(value)
    }
}

fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 128 * 1024,
    }
}
