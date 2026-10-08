//! Bounded cold GDLE server spell programs. This path never invents an account
//! name or player formula: CreatureBeginCast has one CastSpell motion; instant
//! calls have none. Inputs retain the exact native generation and source revision.
use crate::{
    native_magic_assets::{PreparedNativeMagicSpell, prepare_instant_native_magic_spell},
    region_activation::RegionAssetManifest,
};
use bace_dat::{Animation, CollisionSetup, DatArchive, MotionTable, SpellTable};
use bace_storage_codec::{PackGeneration, PackKey, PackLookup};
use bace_types::EntityId;
use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc},
    thread,
};
#[derive(Clone)]
pub struct ServerMagicWork {
    pub token: u64,
    pub actor: EntityId,
    pub source_revision: u64,
    pub source: Arc<bace_content::WeenieV1>,
    pub generation: Arc<PackGeneration>,
    pub spell: u32,
    pub instant: bool,
    pub motion: Option<bace_motion::SourceMotionState>,
}
pub struct PreparedServerMagic {
    pub spell: PreparedNativeMagicSpell,
    pub uses_mana: bool,
}
pub struct ServerMagicResult {
    pub work: Arc<ServerMagicWork>,
    pub result: Result<PreparedServerMagic, String>,
}
pub struct ServerMagicAssets {
    portal: DatArchive,
    spells: SpellTable,
    motions: Option<(u32, MotionTable, BTreeMap<u32, Animation>)>,
}
impl ServerMagicAssets {
    pub fn open(manifest: &RegionAssetManifest) -> Result<Self, String> {
        if manifest.portal_sha256.len() != 64
            || bace_dat::fingerprint(&manifest.portal).map_err(|e| e.to_string())?
                != manifest.portal_sha256
        {
            return Err("unapproved server magic DAT fingerprint".into());
        }
        let mut portal = DatArchive::open(&manifest.portal).map_err(|e| e.to_string())?;
        if portal.header().dataset != 1 {
            return Err("server magic DAT dataset".into());
        }
        let mut budget: usize = 64 * 1024 * 1024;
        let spells = SpellTable::decode(&read(&mut portal, SpellTable::RECORD_ID, &mut budget)?)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            portal,
            spells,
            motions: None,
        })
    }
    pub fn prepare(&mut self, work: &ServerMagicWork) -> Result<PreparedServerMagic, String> {
        if work.token == 0
            || work.actor.0 == 0
            || work.spell == 0
            || work.generation.revision() == 0
            || !work.instant && work.motion.is_none()
        {
            return Err("server magic source identity/bounds".into());
        }
        work.source
            .validate(Default::default())
            .map_err(|e| e.to_string())?;
        let mut budget: usize = 64 * 1024 * 1024;
        let PackLookup::Record(record) = work
            .generation
            .lookup(PackKey {
                namespace: 38,
                id: u64::from(work.spell),
            })
            .map_err(|e| e.to_string())?
        else {
            return Err("server spell row missing".into());
        };
        budget = budget
            .checked_sub(record.bytes().len())
            .ok_or("server spell byte budget")?;
        let bace_content::WorldRecordV1::Spell(row) =
            bace_content_tools::decode_world_record(record.bytes())?
        else {
            return Err("server spell namespace".into());
        };
        if row.id != work.spell {
            return Err("server spell index identity".into());
        }
        let base = self
            .spells
            .spells
            .get(&work.spell)
            .ok_or("server spell DAT entry missing")?;
        let projectile = if matches!(base.meta_type, 2 | 10 | 15) {
            let id = row
                .wcid
                .filter(|id| *id != 0)
                .ok_or("server projectile WCID missing")?;
            let PackLookup::Record(record) = work
                .generation
                .lookup(PackKey {
                    namespace: 1,
                    id: u64::from(id),
                })
                .map_err(|e| e.to_string())?
            else {
                return Err("server projectile template missing".into());
            };
            budget = budget
                .checked_sub(record.bytes().len())
                .ok_or("server projectile byte budget")?;
            let source = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
            if source.weenie_id != id {
                return Err("server projectile identity".into());
            }
            let setup =
                CollisionSetup::decode(&read(&mut self.portal, did(&source, 1)?, &mut budget)?)
                    .map_err(|e| e.to_string())?;
            let shape = crate::world_admission::prepare_collision_shape(&setup, scale(&source)?)?;
            Some((source, shape))
        } else {
            None
        };
        let mut spell = prepare_instant_native_magic_spell(
            work.spell,
            base,
            &row,
            projectile.as_ref().map(|(w, s)| (w, s.clone())),
        )?;
        if !work.instant {
            let motion_id = did(&work.source, 2)?;
            if self
                .motions
                .as_ref()
                .is_none_or(|(id, _, _)| *id != motion_id)
            {
                let table = MotionTable::decode(&read(&mut self.portal, motion_id, &mut budget)?)
                    .map_err(|e| e.to_string())?;
                let mut animations = BTreeMap::new();
                for data in table
                    .cycles
                    .values()
                    .chain(table.modifiers.values())
                    .chain(table.links.values().flat_map(|v| v.values()))
                {
                    for animation in &data.animations {
                        if let std::collections::btree_map::Entry::Vacant(entry) =
                            animations.entry(animation.animation_id)
                        {
                            if animation.animation_id >> 24 != 3 {
                                return Err("server animation namespace".into());
                            }
                            entry.insert(
                                Animation::decode(&read(
                                    &mut self.portal,
                                    animation.animation_id,
                                    &mut budget,
                                )?)
                                .map_err(|e| e.to_string())?,
                            );
                        }
                    }
                }
                self.motions = Some((motion_id, table, animations));
            }
            let (_, table, animations) = self.motions.as_ref().expect("prepared server motion");
            let before = work.motion.ok_or("server motion source missing")?;
            let chain = crate::world_admission::prepare_motion_chain(
                table,
                animations,
                crate::world_admission::MotionChainRequest {
                    style: before.style,
                    current_motion: before.substate,
                    current_speed: before.speed,
                    action: 0x400000d3,
                    action_speed: 2.,
                    scale: scale(&work.source)?,
                    modifiers: &[],
                },
            )?;
            spell.definition.spell.gestures = vec![bace_simulation::PreparedCastGesture {
                gesture: bace_magic::CastGesture {
                    motion: 0x400000d3,
                    minimum_seconds: 2.,
                },
                duration_seconds: 0.,
                motion_chain: Some(chain),
            }];
        }
        Ok(PreparedServerMagic {
            spell,
            uses_mana: work
                .source
                .properties
                .bools
                .iter()
                .find(|p| p.id == 6)
                .is_none_or(|p| p.value),
        })
    }
}
fn did(source: &bace_content::WeenieV1, id: u32) -> Result<u32, String> {
    source
        .properties
        .data_ids
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.value)
        .ok_or_else(|| format!("server magic source DID{id} missing"))
}
fn scale(source: &bace_content::WeenieV1) -> Result<f32, String> {
    let v = source
        .properties
        .floats
        .iter()
        .find(|p| p.id == 39)
        .map_or(1., |p| p.value) as f32;
    if !v.is_finite() || v <= 0. {
        return Err("server magic scale".into());
    }
    Ok(v)
}
fn read(portal: &mut DatArchive, id: u32, budget: &mut usize) -> Result<Vec<u8>, String> {
    let size = portal
        .records()
        .get(&id)
        .ok_or("server magic DAT record missing")?
        .size as usize;
    *budget = budget
        .checked_sub(size)
        .ok_or("server magic DAT byte budget")?;
    portal.read(id).map_err(|e| e.to_string())
}
/// One running plus unclaimed result, with original Arc identity retained.
pub struct ServerMagicWorker {
    sender: Option<mpsc::SyncSender<Arc<ServerMagicWork>>>,
    results: mpsc::Receiver<ServerMagicResult>,
    task: Option<thread::JoinHandle<()>>,
    pending: Option<Arc<ServerMagicWork>>,
}
impl ServerMagicWorker {
    pub fn start(manifest: RegionAssetManifest) -> Result<Self, String> {
        let (sender, inbox) = mpsc::sync_channel::<Arc<ServerMagicWork>>(1);
        let (outbox, results) = mpsc::sync_channel(1);
        let task = thread::Builder::new()
            .name("bace-server-magic".into())
            .spawn(move || {
                let mut assets = None;
                while let Ok(work) = inbox.recv() {
                    let result = (|| {
                        if assets.is_none() {
                            assets = Some(ServerMagicAssets::open(&manifest)?);
                        }
                        assets
                            .as_mut()
                            .expect("opened source assets")
                            .prepare(&work)
                    })();
                    if outbox.send(ServerMagicResult { work, result }).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            sender: Some(sender),
            results,
            task: Some(task),
            pending: None,
        })
    }
    pub fn submit(&mut self, work: Arc<ServerMagicWork>) -> Result<(), Arc<ServerMagicWork>> {
        if work.token == 0
            || self.pending.is_some()
            || self
                .sender
                .as_ref()
                .is_none_or(|s| s.try_send(work.clone()).is_err())
        {
            return Err(work);
        }
        self.pending = Some(work);
        Ok(())
    }
    pub fn poll(&mut self) -> Result<Option<ServerMagicResult>, String> {
        match self.results.try_recv() {
            Ok(done) => {
                if self
                    .pending
                    .as_ref()
                    .is_none_or(|w| !Arc::ptr_eq(w, &done.work))
                {
                    return Err("server magic completion identity".into());
                }
                self.pending = None;
                Ok(Some(done))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err("server magic worker disconnected".into()),
        }
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn try_shutdown(mut self) -> Result<thread::JoinHandle<()>, Box<Self>> {
        if self.pending.is_some() {
            return Err(Box::new(self));
        }
        self.sender.take();
        Ok(self.task.take().expect("retained server magic thread"))
    }
}
