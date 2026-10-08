//! Cold GiveFromEmote item planning/materialization. Reserved GUIDs enter one
//! checked owner batch; no item exists live or durably merely because it is built.
use crate::{
    game_inventory::FrozenInventoryItem,
    region_activation::{RegionAssetManifest, VerifiedRegionAssets},
};
use bace_storage_codec::{EntitySaveV1, PackGeneration, PackKey, PackLookup};
use bace_types::EntityId;
use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc},
    thread,
};
#[derive(Clone)]
pub struct NpcGivePlan {
    pub template: Arc<bace_content::WeenieV1>,
    pub generation: u64,
    pub count: u32,
    pub stack_size: u32,
    pub roots: usize,
    pub palette: i32,
    pub shade: f32,
}
pub struct PreparedNpcGift {
    pub items: Vec<bace_inventory::InventoryItem>,
    pub containers: Vec<bace_inventory::InventoryContainer>,
    pub frozen: Vec<FrozenInventoryItem>,
    pub descriptions: BTreeMap<EntityId, bace_wire::ObjectDescription>,
}
pub enum NpcItemJob {
    Plan {
        source: EntityId,
        ticket: u64,
        generation: Arc<PackGeneration>,
        template: u32,
        count: u32,
        palette: i32,
        shade: f32,
    },
    Build {
        source: EntityId,
        ticket: u64,
        actor: EntityId,
        plan: NpcGivePlan,
        ids: Vec<EntityId>,
    },
}
pub enum NpcItemResult {
    Plan(NpcGivePlan),
    Built(PreparedNpcGift),
}
pub struct NpcItemCompletion {
    pub job: Arc<NpcItemJob>,
    pub result: Result<NpcItemResult, String>,
}
pub struct NpcItemWorker {
    pending: std::cell::Cell<usize>,
    jobs: mpsc::SyncSender<Arc<NpcItemJob>>,
    results: mpsc::Receiver<NpcItemCompletion>,
    thread: thread::JoinHandle<()>,
}
impl NpcItemWorker {
    pub fn start(manifest: RegionAssetManifest) -> Result<Self, String> {
        let (jobs, inbox) = mpsc::sync_channel::<Arc<NpcItemJob>>(1);
        let (outbox, results) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("bace-npc-items".into())
            .spawn(move || {
                let mut assets = None;
                while let Ok(job) = inbox.recv() {
                    let result = match job.as_ref() {
                        NpcItemJob::Plan {
                            generation,
                            template,
                            count,
                            palette,
                            shade,
                            ..
                        } => plan(generation, *template, *count, *palette, *shade)
                            .map(NpcItemResult::Plan),
                        NpcItemJob::Build {
                            actor, plan, ids, ..
                        } => (|| {
                            if assets.is_none() {
                                assets = Some(VerifiedRegionAssets::open(&manifest)?);
                            }
                            build(
                                assets.as_mut().expect("verified NPC item assets"),
                                *actor,
                                plan,
                                ids,
                            )
                            .map(NpcItemResult::Built)
                        })(),
                    };
                    if outbox.send(NpcItemCompletion { job, result }).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            pending: std::cell::Cell::new(0),
            jobs,
            results,
            thread,
        })
    }
    pub fn submit(&self, job: Arc<NpcItemJob>) -> Result<(), Arc<NpcItemJob>> {
        self.jobs
            .try_send(job)
            .map_err(|e| match e {
                mpsc::TrySendError::Full(job) | mpsc::TrySendError::Disconnected(job) => job,
            })
            .map(|()| self.pending.set(self.pending.get() + 1))
    }
    pub fn poll(&self) -> Result<Option<NpcItemCompletion>, String> {
        match self.results.try_recv() {
            Ok(value) => {
                self.pending.set(self.pending.get().saturating_sub(1));
                Ok(Some(value))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err("NPC item worker disconnected".into()),
        }
    }
    pub fn has_pending(&self) -> bool {
        self.pending.get() != 0
    }
    pub fn try_shutdown(self) -> Result<thread::JoinHandle<()>, Box<Self>> {
        if self.has_pending() {
            return Err(Box::new(self));
        }
        drop(self.jobs);
        Ok(self.thread)
    }
    pub fn shutdown(self) -> Result<Vec<NpcItemCompletion>, String> {
        drop(self.jobs);
        let results = self.results.into_iter().collect();
        self.thread.join().map_err(|_| "NPC item worker panicked")?;
        Ok(results)
    }
}
fn plan(
    generation: &PackGeneration,
    template: u32,
    count: u32,
    palette: i32,
    shade: f32,
) -> Result<NpcGivePlan, String> {
    if template == 0 || count == 0 || count > i32::MAX as u32 || !shade.is_finite() {
        return Err("NPC Give quantity/shade".into());
    }
    let PackLookup::Record(record) = generation
        .lookup(PackKey {
            namespace: 1,
            id: u64::from(template),
        })
        .map_err(|e| e.to_string())?
    else {
        return Err("NPC Give template missing".into());
    };
    let value = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
    if value.weenie_id != template
        || crate::generator_preparation::is_creature_template(value.weenie_type)
        || !value.properties.create_list.is_empty()
        || !value.properties.generators.is_empty()
    {
        return Err("NPC Give template requires a named creature/constructor child owner".into());
    }
    let stack_size = if bace_loot::is_stackable(value.weenie_type) {
        u32::try_from(
            value
                .properties
                .ints
                .iter()
                .find(|p| p.id == 11)
                .map_or(1, |p| p.value),
        )
        .map_err(|_| "NPC maximum stack invalid")?
    } else {
        1
    };
    if stack_size == 0 {
        return Err("NPC maximum stack zero".into());
    }
    let roots = count.div_ceil(stack_size) as usize;
    if roots > 1024
        || record
            .bytes()
            .len()
            .checked_mul(roots)
            .is_none_or(|n| n > 32 * 1024 * 1024)
    {
        return Err("NPC Give batch byte/count budget".into());
    }
    Ok(NpcGivePlan {
        template: Arc::new(value),
        generation: generation.revision(),
        count,
        stack_size,
        roots,
        palette,
        shade,
    })
}
fn build(
    assets: &mut VerifiedRegionAssets,
    actor: EntityId,
    plan: &NpcGivePlan,
    ids: &[EntityId],
) -> Result<PreparedNpcGift, String> {
    if actor.0 == 0
        || plan.roots != ids.len()
        || ids.is_empty()
        || ids.len() > 1024
        || ids
            .iter()
            .any(|id| !(0x80000000..=0xfffffffe).contains(&id.0))
        || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
    {
        return Err("NPC Give reserved identity batch".into());
    }
    let mut result = PreparedNpcGift {
        items: vec![],
        containers: vec![],
        frozen: vec![],
        descriptions: BTreeMap::new(),
    };
    let mut remaining = plan.count;
    for &id in ids {
        let mut state = (*plan.template).clone();
        let amount = remaining.min(plan.stack_size);
        remaining -= amount;
        if bace_loot::is_stackable(state.weenie_type) {
            bace_loot::set_treasure_stack(
                &mut state,
                i32::try_from(amount).map_err(|_| "NPC stack numeric bound")?,
            )
            .map_err(|e| format!("NPC stack: {e:?}"))?;
        }
        state
            .properties
            .instance_ids
            .retain(|p| ![1, 2, 3, 6].contains(&p.id));
        state.properties.positions.retain(|p| p.id != 1);
        state.properties.ints.retain(|p| ![10, 53].contains(&p.id));
        if plan.palette > 0 {
            crate::game_inventory::set(&mut state.properties.ints, 3, plan.palette);
        }
        if plan.shade > 0.0 {
            crate::game_inventory::set(&mut state.properties.floats, 12, f64::from(plan.shade));
        }
        let (item, container) = crate::generator_items::prepare_inventory_item(
            &state,
            id,
            1,
            bace_inventory::ItemPlace::Contained {
                container: actor,
                slot: 0,
                equipped: 0,
            },
        )?;
        if item.stack != amount {
            return Err("NPC constructed stack disagrees with source amount".into());
        }
        result.items.push(item);
        result.containers.extend(container);
        result.frozen.push(FrozenInventoryItem {
            corpse: None,
            construction: None,
            source_destination: None,
            enchantments: vec![],
            entity: EntitySaveV1 {
                object_id: id.0,
                template_revision: plan.generation,
                mutation_revision: 1,
                state,
            },
            placement: None,
            persisted_version: 0,
        });
    }
    if remaining != 0 {
        return Err("NPC Give stack plan incomplete".into());
    }
    let sources = result
        .frozen
        .iter()
        .map(|f| crate::visibility_assets::VisibilitySource {
            entity: EntityId(f.entity.object_id),
            incarnation: 1,
            revision: 1,
            source: &f.entity.state,
            equipment: vec![],
            missile_combat: false,
        })
        .collect();
    for description in assets.prepare_visibility_sources(sources)? {
        let mut object = (*description.description).clone();
        object.physics.options.movement = Some(bace_wire::PhysicsMovement::AnimationFrame(101));
        object.physics.options.position = None;
        object.game.options.container = Some(actor.0);
        result
            .descriptions
            .insert(EntityId(object.object_id), object);
    }
    Ok(result)
}
