//! Pinned ACE Vendor.AddDefaultItem. Stock order is insertion order, not GUID
//! order; generated incoming items contribute exactly one to an existing stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VendorStockItem {
    pub id: u32,
    pub template: u32,
    pub stack: Option<i32>,
    pub maximum_stack: Option<i32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendorStockReceipt {
    Added {
        id: u32,
    },
    Stacked {
        retained_id: u32,
        incoming_id: u32,
        stack: i32,
    },
}
impl VendorStockReceipt {
    pub fn retained_id(self) -> u32 {
        match self {
            Self::Added { id } => id,
            Self::Stacked { retained_id, .. } => retained_id,
        }
    }
    pub fn retired_id(self) -> Option<u32> {
        match self {
            Self::Added { .. } => None,
            Self::Stacked { incoming_id, .. } => Some(incoming_id),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendorStockWithdrawal {
    Remaining { id: u32, stack: i32 },
    Removed { item: VendorStockItem },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendorStockError {
    Invalid,
    Capacity,
    Stale,
    RevisionExhausted,
}
#[derive(Clone, Copy, Debug)]
pub struct VendorStockProposal {
    revision: u64,
    item: VendorStockItem,
    receipt: VendorStockReceipt,
}
#[derive(Clone, Copy, Debug)]
pub struct VendorLazyStockProposal {
    revision: u64,
    item: VendorStockItem,
}
impl VendorStockProposal {
    pub fn receipt(&self) -> VendorStockReceipt {
        self.receipt
    }
}
/// Single simulation-owner vendor stock. Full immutable item snapshots remain
/// with the admission owner, keyed by the receipt's retained identity. On a
/// stack receipt the incoming identity must never become a spawned child.
#[derive(Clone)]
pub struct VendorStock {
    items: Vec<VendorStockItem>,
    capacity: usize,
    revision: u64,
}
impl VendorStock {
    pub fn new(capacity: usize) -> Result<Self, VendorStockError> {
        if !(1..=4096).contains(&capacity) {
            return Err(VendorStockError::Capacity);
        }
        Ok(Self {
            items: Vec::with_capacity(capacity),
            capacity,
            revision: 0,
        })
    }
    pub fn items(&self) -> &[VendorStockItem] {
        &self.items
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// The first-Use load is itself an owner transition, including when no
    /// authored Shop rows exist. Its durable marker therefore has revision 1.
    pub fn mark_lazy_loaded(&mut self) -> Result<(), VendorStockError> {
        if self.revision != 0 || !self.items.is_empty() {
            return Err(VendorStockError::Stale);
        }
        self.revision = 1;
        Ok(())
    }
    /// ACE Vendor.LoadInventory inserts every authored Shop row in source order,
    /// even when another row has the same WCID. Generator AddDefaultItem uses
    /// a separate first-stack merge rule.
    pub fn propose_lazy(
        &self,
        item: VendorStockItem,
    ) -> Result<VendorLazyStockProposal, VendorStockError> {
        if item.id == 0
            || item.template == 0
            || item.stack.is_some_and(|count| count <= 0)
            || item.maximum_stack.is_some_and(|count| count <= 0)
            || self.items.iter().any(|existing| existing.id == item.id)
        {
            return Err(VendorStockError::Invalid);
        }
        if self.items.len() == self.capacity {
            return Err(VendorStockError::Capacity);
        }
        self.revision
            .checked_add(1)
            .ok_or(VendorStockError::RevisionExhausted)?;
        Ok(VendorLazyStockProposal {
            revision: self.revision,
            item,
        })
    }
    pub fn commit_lazy(
        &mut self,
        proposal: VendorLazyStockProposal,
    ) -> Result<u32, VendorStockError> {
        if proposal.revision != self.revision {
            return Err(VendorStockError::Stale);
        }
        self.propose_lazy(proposal.item)?;
        self.items.push(proposal.item);
        self.revision += 1;
        Ok(proposal.item.id)
    }
    pub fn propose_default(
        &self,
        item: VendorStockItem,
    ) -> Result<VendorStockProposal, VendorStockError> {
        if item.id == 0
            || item.template == 0
            || item.stack.is_some_and(|v| v <= 0)
            || item.maximum_stack.is_some_and(|v| v <= 0)
            || self.items.iter().any(|v| v.id == item.id)
        {
            return Err(VendorStockError::Invalid);
        }
        if self.revision == u64::MAX {
            return Err(VendorStockError::RevisionExhausted);
        }
        let receipt = if let Some(existing) = self.items.iter().find(|v| {
            v.template == item.template && v.stack.unwrap_or(1) < v.maximum_stack.unwrap_or(1)
        }) {
            VendorStockReceipt::Stacked {
                retained_id: existing.id,
                incoming_id: item.id,
                stack: existing.stack.unwrap_or(1) + 1,
            }
        } else {
            if self.items.len() == self.capacity {
                return Err(VendorStockError::Capacity);
            }
            VendorStockReceipt::Added { id: item.id }
        };
        Ok(VendorStockProposal {
            revision: self.revision,
            item,
            receipt,
        })
    }
    pub fn commit_default(
        &mut self,
        proposal: VendorStockProposal,
    ) -> Result<VendorStockReceipt, VendorStockError> {
        if proposal.revision != self.revision {
            return Err(VendorStockError::Stale);
        }
        let checked = self.propose_default(proposal.item)?;
        if checked.receipt != proposal.receipt {
            return Err(VendorStockError::Stale);
        }
        match proposal.receipt {
            VendorStockReceipt::Added { .. } => self.items.push(proposal.item),
            VendorStockReceipt::Stacked {
                retained_id, stack, ..
            } => {
                self.items
                    .iter_mut()
                    .find(|v| v.id == retained_id)
                    .ok_or(VendorStockError::Stale)?
                    .stack = Some(stack);
            }
        }
        self.revision += 1;
        Ok(proposal.receipt)
    }
    /// Withdraw exactly the recorded generator contribution. A merged stock
    /// member remains alive while other generator contributions still own units.
    pub fn withdraw_default(
        &mut self,
        id: u32,
        units: u32,
        expected_revision: u64,
    ) -> Result<VendorStockWithdrawal, VendorStockError> {
        if self.revision != expected_revision {
            return Err(VendorStockError::Stale);
        }
        let index = self
            .items
            .iter()
            .position(|v| v.id == id)
            .ok_or(VendorStockError::Invalid)?;
        let count = self.items[index].stack.unwrap_or(1);
        let units = i32::try_from(units).map_err(|_| VendorStockError::Invalid)?;
        if units <= 0 || units > count {
            return Err(VendorStockError::Invalid);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(VendorStockError::RevisionExhausted)?;
        let result = if units == count {
            VendorStockWithdrawal::Removed {
                item: self.items.remove(index),
            }
        } else {
            let stack = count - units;
            self.items[index].stack = Some(stack);
            VendorStockWithdrawal::Remaining { id, stack }
        };
        self.revision = revision;
        Ok(result)
    }
    pub fn remove(
        &mut self,
        id: u32,
        expected_revision: u64,
    ) -> Result<VendorStockItem, VendorStockError> {
        if expected_revision != self.revision {
            return Err(VendorStockError::Stale);
        }
        let index = self
            .items
            .iter()
            .position(|v| v.id == id)
            .ok_or(VendorStockError::Invalid)?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(VendorStockError::RevisionExhausted)?;
        let item = self.items.remove(index);
        self.revision = revision;
        Ok(item)
    }
}
