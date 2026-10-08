//! One bounded cold split preparation. The accepted template and DAT closure are
//! pinned before proposal; immutable factory state survives all durable retries.
use super::*;
use crate::{
    inventory_service::{SplitPreparationInput, prepare_split_request},
    player_entry::PreparedEntryAppearanceAssets,
    stack_factory::PreparedStack,
};
use bace_content::WeenieV1;
use bace_inventory::InventoryAuthority;
use bace_storage_codec::{ItemPlacementV2, PackGeneration, PackKey, PackLookup};
pub(super) struct Prepared {
    pub request: Option<InventoryCommandKind>,
    pub external: Vec<crate::game_inventory::FrozenInventoryItem>,
    pub fresh: Option<PreparedStack>,
    pub appearance: Option<PreparedEntryAppearanceAssets>,
    pub equipment_dat: Option<Arc<crate::player_assets::PreparedAvatarDat>>,
}
impl Prepared {
    pub fn ordinary() -> Self {
        Self {
            request: None,
            external: vec![],
            fresh: None,
            appearance: None,
            equipment_dat: None,
        }
    }
}
pub(super) fn authority(actor: EntityId) -> InventoryAuthority {
    InventoryAuthority {
        actor,
        busy: false,
        in_range: false,
        clear_path: false,
        geometry_ready: false,
        drop_validated: false,
        source_view: None,
        destination_view: None,
        new_item: None,
    }
}
pub(super) fn owned_source<'a>(
    actor: EntityId,
    request: InventoryRequest,
    mut get: impl FnMut(u32) -> Option<(&'a WeenieV1, &'a ItemPlacementV2)>,
) -> Result<WeenieV1, String> {
    let (source, destination) = match request {
        InventoryRequest::Move {
            item, container, ..
        }
        | InventoryRequest::SplitToContainer {
            item, container, ..
        } => (item, container),
        InventoryRequest::Merge { source, target, .. } => (source, target),
        _ => return Err("inventory world/equipment preparation required".into()),
    };
    for mut id in [source, destination] {
        let mut valid = false;
        for _ in 0..1024 {
            if id == actor {
                valid = true;
                break;
            }
            let (_, placement) = get(id.0)
                .ok_or("external inventory item requires accepted world/storage authority")?;
            match placement {
                ItemPlacementV2::Contained {
                    container,
                    equipped: 0,
                    ..
                } => id = EntityId(*container),
                _ => {
                    return Err(
                        "equipped/world inventory requires prepared physical transition".into(),
                    );
                }
            }
        }
        if !valid {
            return Err("inventory ancestry cycle/capacity".into());
        }
    }
    get(source.0)
        .map(|(state, _)| state.clone())
        .ok_or("source item baseline missing".into())
}
pub(super) async fn prepare_split(
    store: bace_db_postgres::PgStore,
    generation: Arc<PackGeneration>,
    manifest: crate::region_activation::RegionAssetManifest,
    context: ActionContext,
    request: InventoryRequest,
    source: WeenieV1,
) -> Result<Prepared, String> {
    let ids = store
        .allocate_dynamic_ids(1)
        .await
        .map_err(|e| e.to_string())?;
    if ids.len() != 1 {
        return Err("split identity allocation count".into());
    }
    let id = EntityId(ids[0]);
    // There is exactly one retained cold future in InventoryRuntime, not one
    // task per item. Mapping faults and archive I/O occur only on cold capacity.
    tokio::task::spawn_blocking(move || {
        let PackLookup::Record(record) = generation
            .lookup(PackKey {
                namespace: 1,
                id: u64::from(source.weenie_id),
            })
            .map_err(|e| e.to_string())?
        else {
            return Err("accepted split template is missing".into());
        };
        if record.bytes().len() > 64 * 1024 * 1024 {
            return Err("split template byte capacity".into());
        }
        let template = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
        let mut authority = authority(context.actor);
        authority.new_item = Some(id);
        let (request, fresh) = prepare_split_request(SplitPreparationInput {
            context,
            request,
            authority,
            template: &template,
            template_revision: generation.revision(),
            fresh_id: id,
            source: &source,
            source_vendor: false,
            destination_corpse: false,
            wield_requirements_met: false,
            drop: None,
        })?;
        let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
        let appearance = assets.prepare_entry_appearance(&[&fresh.frozen.entity.state])?;
        Ok(Prepared {
            request: Some(InventoryCommandKind::ProposeOwned(Box::new(request))),
            external: vec![],
            fresh: Some(fresh),
            appearance: Some(appearance),
            equipment_dat: None,
        })
    })
    .await
    .map_err(|e| format!("split cold preparation: {e}"))?
}
