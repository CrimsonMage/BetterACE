//! Fresh splits use accepted template factory data, never the source instance.
use crate::stack_factory::{PreparedStack, prepare_split_stack};
use bace_content::WeenieV1;
use bace_gameplay_api::{ActionContext, InventoryRequest};
use bace_inventory::{InventoryAuthority, ItemPlace, StackSplitPreparation};
use bace_simulation::{InventoryPreparedRequest, PreparedStackDrop};
use bace_types::EntityId;
pub struct SplitPreparationInput<'a> {
    pub context: ActionContext,
    pub request: InventoryRequest,
    pub authority: InventoryAuthority,
    pub template: &'a WeenieV1,
    pub template_revision: u64,
    pub fresh_id: EntityId,
    pub source: &'a WeenieV1,
    pub source_vendor: bool,
    pub destination_corpse: bool,
    pub wield_requirements_met: bool,
    pub drop: Option<Box<PreparedStackDrop>>,
}
pub fn prepare_split_request(
    input: SplitPreparationInput<'_>,
) -> Result<(InventoryPreparedRequest, PreparedStack), String> {
    if input.template.weenie_id != input.source.weenie_id
        || input.authority.actor != input.context.actor
        || input.authority.new_item != Some(input.fresh_id)
    {
        return Err("split template/authority identity mismatch".into());
    }
    let (amount, destination) = match input.request {
        InventoryRequest::SplitToContainer {
            container,
            placement,
            amount,
            ..
        } => (
            amount,
            ItemPlace::Contained {
                container,
                slot: u32::try_from(placement).map_err(|_| "split placement negative")?,
                equipped: 0,
            },
        ),
        InventoryRequest::SplitToWorld { amount, .. } => (amount, ItemPlace::World),
        InventoryRequest::SplitToWield {
            location, amount, ..
        } => (
            amount,
            ItemPlace::Contained {
                container: input.context.actor,
                slot: 0,
                equipped: location,
            },
        ),
        _ => return Err("not a split request".into()),
    };
    if matches!(destination, ItemPlace::World) != input.drop.is_some() {
        return Err("split world geometry missing or unexpected".into());
    }
    let fresh = prepare_split_stack(
        input.template,
        input.template_revision,
        input.fresh_id,
        u32::try_from(amount).map_err(|_| "split amount negative")?,
        destination,
        input.wield_requirements_met,
    )?;
    let prepared = StackSplitPreparation {
        fresh: fresh.item.clone(),
        source_stackable: bace_loot::is_stackable(input.source.weenie_type),
        source_stuck: input
            .source
            .properties
            .bools
            .iter()
            .any(|p| p.id == 1 && p.value),
        source_vendor: input.source_vendor,
        destination_corpse: input.destination_corpse,
    };
    Ok((
        InventoryPreparedRequest {
            context: input.context,
            request: input.request,
            authority: input.authority,
            split: Some(prepared),
            drop: input.drop,
        },
        fresh,
    ))
}
