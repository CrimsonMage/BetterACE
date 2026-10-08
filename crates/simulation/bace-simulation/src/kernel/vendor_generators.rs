//! Vendor stock has one economy owner; generated merges retain the actual item
//! identity and the source contribution, never a phantom incoming child.
use super::*;
use crate::GeneratorServiceError as G;
use bace_economy::{VendorStock, VendorStockWithdrawal};
use bace_gameplay_api::{GeneratorSpawnKey, GeneratorSpawnMember};
use std::{collections::BTreeMap, sync::Arc};
pub(super) struct GeneratedVendor {
    pub stock: VendorStock,
    pub items: BTreeMap<u32, Arc<bace_content::WeenieV1>>,
    pub contents: BTreeMap<u32, crate::VendorContents>,
    pub display_quantities: BTreeMap<u32, i32>,
    pub lazy_loaded: bool,
    pub pending_lazy: Option<Box<super::vendor_lazy::PendingVendorLazy>>,
    pub committed_lazy: Option<crate::vendor_trees::VendorLazyStockTicket>,
    pub pending_buy: Option<Box<super::vendor_buy::VendorBuyReservation>>,
    pub marker_version: i64,
    pub marker_stock_revision: u64,
}
impl Kernel {
    pub(super) fn generated_vendor_reserves_identity(&self, id: EntityId) -> bool {
        self.generated_vendors.contains_key(&id)
            || self.generated_vendors.values().any(|vendor| {
                vendor.items.contains_key(&id.0)
                    || vendor.pending_lazy.as_ref().is_some_and(|pending| {
                        pending.ticket.marker == id || pending.ticket.item_ids.contains(&id)
                    })
                    || vendor.pending_buy.as_ref().is_some_and(|pending| {
                        pending
                            .inventory
                            .proposal
                            .changes
                            .iter()
                            .any(|change| change.before.is_none() && change.after.id == id)
                    })
            })
    }
    pub fn register_generator_vendor(
        &mut self,
        vendor: EntityId,
        capacity: usize,
    ) -> Result<(), G> {
        if self.generated_vendors.contains_key(&vendor) || self.generated_vendors.len() >= 4096 {
            return Err(G::Capacity);
        }
        self.generated_vendors.insert(
            vendor,
            GeneratedVendor {
                stock: VendorStock::new(capacity).map_err(|_| G::Capacity)?,
                items: BTreeMap::new(),
                contents: BTreeMap::new(),
                display_quantities: BTreeMap::new(),
                lazy_loaded: false,
                pending_lazy: None,
                committed_lazy: None,
                pending_buy: None,
                marker_version: 0,
                marker_stock_revision: 0,
            },
        );
        Ok(())
    }
    pub fn generated_vendor_stock(&self, vendor: EntityId) -> Option<&VendorStock> {
        self.generated_vendors.get(&vendor).map(|v| &v.stock)
    }
    pub fn vendor_lazy_pending(&self, vendor: EntityId) -> bool {
        self.generated_vendors
            .get(&vendor)
            .is_some_and(|owner| owner.pending_lazy.is_some())
    }
    pub fn vendor_stock_nonempty(&self, vendor: EntityId) -> bool {
        self.generated_vendors
            .get(&vendor)
            .is_some_and(|owner| !owner.stock.items().is_empty())
    }
    pub fn vendor_stock_durable(&self, vendor: EntityId) -> bool {
        self.generated_vendors.get(&vendor).is_some_and(|owner| {
            owner.pending_lazy.is_none()
                && owner.pending_buy.is_none()
                && owner.lazy_loaded
                && owner.marker_version > 0
                && owner.marker_stock_revision >= owner.stock.revision()
                && owner.committed_lazy.as_ref().is_some_and(|ticket| {
                    ticket.vendor == vendor
                        && ticket.stock_revision == owner.stock.revision()
                        && ticket
                            .item_ids
                            .iter()
                            .all(|id| owner.items.contains_key(&id.0))
                })
        })
    }
    pub fn generated_vendor_item(
        &self,
        vendor: EntityId,
        item: u32,
    ) -> Option<&Arc<bace_content::WeenieV1>> {
        self.generated_vendors.get(&vendor)?.items.get(&item)
    }
    pub fn admit_generated_vendor_stock(
        &mut self,
        key: GeneratorSpawnKey,
        items: &[(EntityId, Arc<bace_content::WeenieV1>)],
    ) -> Result<(), G> {
        let trees: Vec<_> = items
            .iter()
            .map(|(root, template)| crate::PreparedVendorTree {
                root: *root,
                template: template.clone(),
                items: vec![],
                containers: vec![],
                templates: BTreeMap::new(),
            })
            .collect();
        self.admit_generated_vendor_trees(key, &trees).map(|_| ())
    }
    pub(super) fn withdraw_generated_vendor_member(
        &mut self,
        vendor: EntityId,
        member: GeneratorSpawnMember,
    ) -> Result<bool, G> {
        let Some(owner) = self.generated_vendors.get_mut(&vendor) else {
            return Ok(false);
        };
        if owner.pending_lazy.is_some() || owner.pending_buy.is_some() || owner.lazy_loaded {
            return Err(G::Busy);
        }
        if !owner.items.contains_key(&member.entity.0) {
            return Ok(false);
        }
        let mut stock = owner.stock.clone();
        let receipt = stock
            .withdraw_default(member.entity.0, member.contribution, stock.revision())
            .map_err(|_| G::Invalid)?;
        match receipt {
            VendorStockWithdrawal::Remaining { id, stack } => {
                let mut template = (**owner.items.get(&id).ok_or(G::Invalid)?).clone();
                bace_loot::set_treasure_stack(&mut template, stack).map_err(|_| G::Invalid)?;
                owner.items.insert(id, Arc::new(template));
            }
            VendorStockWithdrawal::Removed { item } => {
                owner.items.remove(&item.id);
                owner.display_quantities.remove(&item.id);
                if let Some(contents) = owner.contents.remove(&item.id) {
                    for child in contents.items {
                        owner.items.remove(&child.id.0);
                    }
                }
            }
        }
        owner.stock = stock;
        Ok(true)
    }
}
