//! Bounded cold avatar/entry preparation. The worker owns verified DAT archives;
//! gameplay and canonical session sequence owners remain outside this thread.
mod lane;
use crate::{
    character_assets::PreparedCharacterAssets,
    game_login::LoadedPlayer,
    player_assets::{PlayerColdAssets, PlayerColdPolicy, prepare_loaded_avatar},
    player_entry::PreparedEntryAppearanceAssets,
    region_activation::{RegionAssetManifest, VerifiedRegionAssets},
};
use bace_gameplay_api::{ActionContext, CharacterBinding, EnchantmentProjection};
use bace_session::SessionKey;
use std::{collections::BTreeMap, sync::Arc, thread};

pub struct PlayerPreparationRequest {
    pub generation: Arc<bace_storage_codec::PackGeneration>,
    pub correlation: u64,
    pub loaded: Arc<LoadedPlayer>,
    /// Exact immutable geometry retained by the caller's region activation fence.
    pub geometry: Arc<bace_physics::GeometryRegion>,
    pub spell_rows: Arc<BTreeMap<u32, bace_content::SpellRowV1>>,
    pub component_templates: Arc<[u32]>,
    pub projectile_shapes: Arc<BTreeMap<u32, Arc<bace_physics::CollisionShape>>>,
    pub policy: PlayerColdPolicy,
    pub now_unix_millis: u64,
}
pub struct PreparedPlayerCold {
    pub admission: bace_simulation::PreparedPlayerAdmission,
    pub appearance: PreparedEntryAppearanceAssets,
    pub character_assets: Arc<PreparedCharacterAssets>,
    pub spell_table: Arc<bace_dat::SpellTable>,
    pub enchantments: Vec<EnchantmentProjection>,
}
pub struct PlayerPreparationCompletion {
    pub correlation: u64,
    pub key: SessionKey,
    pub binding: CharacterBinding,
    pub result: Result<PreparedPlayerCold, String>,
}
pub struct BindingMotionPreparationRequest {
    pub correlation: u64,
    pub context: ActionContext,
    pub before_revision: u64,
    pub source: bace_content::WeenieV1,
    pub source_object: bace_types::EntityId,
    pub current: bace_motion::SourceMotionState,
}
pub struct BindingMotionPreparationCompletion {
    pub correlation: u64,
    pub context: ActionContext,
    pub before_revision: u64,
    pub result: Result<PreparedBindingMotionData, String>,
}
pub struct PreparedBindingMotionData {
    pub style: Option<Arc<bace_motion::PreparedMotionChain>>,
    pub motion: Arc<bace_motion::PreparedMotionChain>,
    pub seconds: f64,
}
enum PreparationJob {
    Admission(PlayerPreparationRequest),
    BindingMotion(Box<BindingMotionPreparationRequest>),
}
pub enum PlayerPreparationResult {
    Admission(Box<PlayerPreparationCompletion>),
    BindingMotion(BindingMotionPreparationCompletion),
}
pub struct PlayerPreparationWorker {
    lane: lane::Lane<PreparationJob, PlayerPreparationResult>,
    admission_waiting: bool,
    capacity: usize,
}
impl PlayerPreparationWorker {
    /// At most four requests including executing and completed-but-unclaimed work.
    /// Each existing avatar/appearance decoder enforces its 64 MiB source closure;
    /// immutable shared region/spell inputs are never copied into a second owner.
    pub fn start(manifest: RegionAssetManifest, capacity: usize) -> Result<Self, String> {
        let mut assets = None;
        let lane = lane::Lane::start(capacity, move |request: PreparationJob| {
            let assets = assets.get_or_insert_with(|| VerifiedRegionAssets::open(&manifest));
            match request {
                PreparationJob::Admission(request) => {
                    let correlation = request.correlation;
                    let key = request.loaded.key;
                    let binding = request.loaded.binding;
                    let result = assets
                        .as_mut()
                        .map_err(|e| e.clone())
                        .and_then(|assets| prepare(assets, request));
                    PlayerPreparationResult::Admission(Box::new(PlayerPreparationCompletion {
                        correlation,
                        key,
                        binding,
                        result,
                    }))
                }
                PreparationJob::BindingMotion(request) => {
                    let result = assets
                        .as_mut()
                        .map_err(|e| e.clone())
                        .and_then(|assets| {
                            assets.prepare_binding_motion(&request.source, request.current)
                        })
                        .map(|prepared| PreparedBindingMotionData {
                            style: prepared.style,
                            motion: prepared.motion,
                            seconds: prepared.seconds,
                        });
                    PlayerPreparationResult::BindingMotion(BindingMotionPreparationCompletion {
                        correlation: request.correlation,
                        context: request.context,
                        before_revision: request.before_revision,
                        result,
                    })
                }
            }
        })?;
        Ok(Self {
            lane,
            admission_waiting: false,
            capacity,
        })
    }
    pub fn try_submit(
        &mut self,
        request: PlayerPreparationRequest,
    ) -> Result<(), Box<PlayerPreparationRequest>> {
        if request.loaded.inventory.len() > 1023
            || request.component_templates.len() > 4096
            || request.spell_rows.len() > 65536
            || request.projectile_shapes.len() > 1024
            || request.policy.staff.binding != request.loaded.binding
            || request.loaded.binding.actor.0 != request.loaded.player.player.entity.object_id
        {
            return Err(Box::new(request));
        }
        if self.pending() >= self.capacity {
            self.admission_waiting = true;
            return Err(Box::new(request));
        }
        match self
            .lane
            .try_submit(request.correlation, PreparationJob::Admission(request))
        {
            Ok(()) => {
                self.admission_waiting = false;
                Ok(())
            }
            Err(PreparationJob::Admission(request)) => {
                self.admission_waiting = true;
                Err(Box::new(request))
            }
            Err(PreparationJob::BindingMotion(_)) => unreachable!("admission job identity"),
        }
    }
    pub fn try_submit_binding(
        &mut self,
        request: BindingMotionPreparationRequest,
    ) -> Result<(), Box<BindingMotionPreparationRequest>> {
        if request.correlation == 0
            || request.source_object != request.context.actor
            || self.admission_waiting
            || self.pending() >= self.capacity.saturating_sub(1).max(1)
        {
            return Err(Box::new(request));
        }
        match self.lane.try_submit(
            request.correlation,
            PreparationJob::BindingMotion(Box::new(request)),
        ) {
            Ok(()) => Ok(()),
            Err(PreparationJob::BindingMotion(request)) => Err(request),
            Err(PreparationJob::Admission(_)) => unreachable!("binding job identity"),
        }
    }
    pub fn try_recv(&mut self) -> Result<Option<PlayerPreparationResult>, String> {
        self.lane.try_recv()
    }
    pub fn pending(&self) -> usize {
        self.lane.pending()
    }
    /// Only after all completions are claimed. Join this handle on blocking capacity.
    pub fn try_shutdown(self) -> Result<thread::JoinHandle<()>, Box<Self>> {
        match self.lane.try_shutdown() {
            Ok(thread) => Ok(thread),
            Err(lane) => Err(Box::new(Self {
                lane: *lane,
                admission_waiting: self.admission_waiting,
                capacity: self.capacity,
            })),
        }
    }
}
fn prepare(
    assets: &mut VerifiedRegionAssets,
    request: PlayerPreparationRequest,
) -> Result<PreparedPlayerCold, String> {
    request
        .loaded
        .player
        .validate()
        .map_err(|e| e.to_string())?;
    let avatar = assets.prepare_avatar_dat(&request.loaded.player.player.entity.state)?;
    let mut admission = prepare_loaded_avatar(
        &request.loaded,
        PlayerColdAssets {
            avatar: &avatar,
            geometry: &request.geometry,
            spell_rows: &request.spell_rows,
            component_templates: &request.component_templates,
            projectile_shapes: &request.projectile_shapes,
        },
        request.policy,
        request.now_unix_millis,
    )?;
    admission.server_magic = assets
        .prepare_proc_magic(
            &request.generation,
            &avatar.spells,
            &admission.magic_damage,
            &admission.physical,
        )?
        .batch;
    let sources: Vec<_> = std::iter::once(&request.loaded.player.player.entity.state)
        .chain(
            request
                .loaded
                .inventory
                .iter()
                .map(|item| &item.entity.state),
        )
        .collect();
    let appearance = assets.prepare_entry_appearance(&sources)?;
    let enchantments = admission
        .state
        .enchantments
        .as_ref()
        .ok_or("prepared avatar has no registry")?
        .entries()
        .iter()
        .map(|entry| {
            crate::enchantment_saves::prepare_enchantment_projection(
                entry,
                crate::player_assets::player_enchantment_definition(
                    entry.spell,
                    &avatar.spells,
                    &request.spell_rows,
                ),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(PreparedPlayerCold {
        admission,
        appearance,
        character_assets: avatar.character,
        spell_table: avatar.spells,
        enchantments,
    })
}
