use super::*;
use crate::{
    GeneratorAction, GeneratorCommand, GeneratorCommandOutcome, GeneratorServiceError as G,
};
impl Kernel {
    pub fn take_generator_outcome(&mut self) -> Option<GeneratorCommandOutcome> {
        self.generator_outcomes.pop_front()
    }
    pub(super) fn handle_generator_command(
        &mut self,
        command: GeneratorCommand,
    ) -> GeneratorCommandOutcome {
        let mut admission = None;
        let mut request = None;
        let result = match command.action {
            GeneratorAction::AdmitWithScripts { sources, action } => {
                return self.admit_generator_script_batch(command.correlation, sources, *action);
            }
            GeneratorAction::QuiesceRegions => self.quiesce_regions().map_err(|_| G::Busy),
            GeneratorAction::ConfirmRegionUnload(receipt) => self
                .confirm_region_unload_saved(&receipt)
                .map_err(|_| G::Stale),
            GeneratorAction::RetryRegionUnload { operation } => {
                self.retry_region_unload(operation).map_err(|_| G::Stale)
            }
            GeneratorAction::RequestRegion {
                landblock,
                permanent,
            } => self
                .request_region(landblock, permanent)
                .map(|_| ())
                .map_err(|e| match e {
                    crate::ResidencyError::Capacity => G::Capacity,
                    crate::ResidencyError::Busy => G::Busy,
                    _ => G::Stale,
                }),
            GeneratorAction::RetryRegion { landblock, epoch } => self
                .retry_region_preparation(landblock, epoch)
                .map_err(|_| G::Stale),
            GeneratorAction::AdmitResidentRegion {
                region,
                dungeon,
                keep_alive,
            } => self.admit_resident_region(*region, dungeon, keep_alive),
            GeneratorAction::SupplyId(id) => self.supply_generator_id(id),
            GeneratorAction::DiscardUnusedIdsAfterDrain => {
                self.discard_unused_generator_ids_after_drain()
            }
            GeneratorAction::SupplyRequestIds { key, expected, ids } => self
                .supply_generator_request_ids(key, expected, &ids)
                .and_then(|_| self.refresh_generator_request(key))
                .map(|refreshed| request = Some(refreshed)),
            GeneratorAction::RefreshRequest(key) => self
                .refresh_generator_request(key)
                .map(|refreshed| request = Some(refreshed)),
            GeneratorAction::SetDay(value) => {
                self.set_generator_day(value);
                Ok(())
            }
            GeneratorAction::AdmitCreature { key, loadout } => {
                self.admit_generated_creature(key, *loadout)
            }
            GeneratorAction::AdmitContainedCreature { key, prepared } => self
                .admit_contained_creature(key, *prepared)
                .map(|accepted| admission = Some(accepted)),
            GeneratorAction::AdmitContainedForest { key, prepared } => self
                .admit_contained_forest(key, *prepared)
                .map(|accepted| admission = Some(accepted)),
            GeneratorAction::CompleteRetirement(receipt) => self
                .confirm_generated_retirement(&receipt)
                .map(|_| ())
                .map_err(|_| G::Busy),
            GeneratorAction::RejectRetirement { operation } => self
                .reject_generated_retirement(operation)
                .map_err(|_| G::Busy),
            GeneratorAction::RetryRetirement { operation } => self
                .retry_generated_retirement(operation)
                .map_err(|_| G::Busy),
            GeneratorAction::ReserveRequestIds { key, total } => self
                .reserve_generator_request_ids(key, total)
                .and_then(|_| self.retry_generator_request(key)),
            GeneratorAction::RetryRequest(key) => self.retry_generator_request(key),
            GeneratorAction::AdmitRegion(region) => self.admit_generator_region(*region),
            GeneratorAction::Control { identity, control } => {
                self.generator_control(identity, control)
            }
            GeneratorAction::SpawnReceipt(receipt) => {
                if matches!(&receipt.result,bace_gameplay_api::GeneratorSpawnResult::Completed{members,..} if !members.is_empty())
                {
                    Err(G::Invalid)
                } else {
                    self.confirm_generator_spawn(receipt)
                }
            }
            GeneratorAction::AdmitItems {
                key,
                items,
                containers,
                shapes,
            } => match shapes {
                Some(shapes) => {
                    self.admit_generated_world_inventory(key, &items, &containers, &shapes)
                }
                None => self.admit_generated_inventory(key, &items, &containers),
            },
            GeneratorAction::AdmitItemTrees {
                key,
                items,
                containers,
                roots,
                shapes,
            } => self
                .admit_generated_item_trees(key, &items, &containers, &roots, shapes.as_deref())
                .map(|accepted| admission = Some(accepted)),
            GeneratorAction::AdmitMixedTrees { key, roots } => self
                .admit_generated_mixed_trees(key, roots)
                .map(|accepted| admission = Some(accepted)),
            GeneratorAction::AdmitStockTrees { key, trees } => self
                .admit_generated_vendor_trees(key, &trees)
                .map(|accepted| admission = Some(accepted)),
            GeneratorAction::AdmitStock { key, items } => {
                self.admit_generated_vendor_stock(key, &items)
            }
            GeneratorAction::Acquire { receipt, transient } => self
                .confirm_generated_inventory_committed(&receipt, &transient)
                .map(|_| ())
                .map_err(|e| {
                    if e == bace_gameplay_api::InventoryRejection::Capacity {
                        G::Capacity
                    } else {
                        G::Busy
                    }
                }),
            GeneratorAction::RejectAcquisition { operation } => {
                self.reject_inventory(operation).map_err(|_| G::Busy)
            }
        };
        GeneratorCommandOutcome {
            correlation: command.correlation,
            result,
            admission,
            request,
        }
    }
}
