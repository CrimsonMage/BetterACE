//! Frozen ACE death-table/create-list context. Generated items are full immutable
//! template snapshots; valuable-operation adapters preserve every property family.
use super::*;
use bace_content::WeenieV1;
use bace_gameplay_api::{RareDecision, RareKillContext};
use bace_loot::{DeathTreasure, RareEvaluator};
use bace_random::{Domain, RandomRoot};
use std::sync::Arc;
#[derive(Clone)]
pub struct AceCreatureLootPolicy {
    pub source: Arc<WeenieV1>,
    pub initial_items: Vec<WeenieV1>,
    pub initial_parents: Vec<Option<usize>>,
    pub initial_ids: Vec<EntityId>,
    pub treasure: Option<Arc<DeathTreasure>>,
    pub templates: Arc<BTreeMap<u32, Arc<WeenieV1>>>,
    pub profile_id: u32,
    pub profile_revision: [u8; 32],
    pub content_generation: [u8; 32],
    pub rare: Option<Arc<RareEvaluator>>,
    pub rare_profile_revision: Option<[u8; 32]>,
    pub creature_level: u32,
}
pub(super) struct AceContext {
    pub policy: AceCreatureLootPolicy,
    pub event: [u8; 16],
    pub originals: Vec<(EntityId, WeenieV1)>,
}
impl native::NativeLoot {
    pub(super) fn prepare_ace(
        &self,
        actor: EntityId,
        owner: Option<EntityId>,
        world: &World,
        characters: &Characters,
        tick: u64,
    ) -> Result<Option<NativeDeathLoot>, PveError> {
        let ctx = self.aces.get(&actor).ok_or(PveError::InvalidProfile)?;
        let p = &ctx.policy;
        let root = self.root.as_ref().ok_or(PveError::InvalidProfile)?;
        let mut random = root
            .event_stream(ctx.event, Domain::OrdinaryLoot)
            .and_then(|r| r.fork(b"ace-death", u64::from(p.profile_id)))
            .map_err(|_| PveError::InvalidRandom)?;
        let mut items = if let Some(treasure) = &p.treasure {
            treasure
                .generate(&mut random)
                .map_err(|_| PveError::InvalidProfile)?
        } else {
            vec![]
        };
        if p.initial_items.len() != p.initial_parents.len()
            || p.initial_parents
                .iter()
                .enumerate()
                .any(|(i, parent)| parent.is_some_and(|p| p >= i))
        {
            return Err(PveError::InvalidProfile);
        }
        let offset = items.len();
        let mut parents = vec![None; offset];
        parents.extend(
            p.initial_parents
                .iter()
                .map(|parent| parent.map(|index| index + offset)),
        );
        items.extend(p.initial_items.iter().cloned());
        let rows: Vec<_> = p
            .source
            .properties
            .create_list
            .iter()
            .filter(|r| {
                r.destination_type & 1 != 0
                    || (r.destination_type & 8 != 0 && r.destination_type & 2 == 0)
            })
            .collect();
        let selected = bace_loot::generate_create_list_selection(
            &rows.iter().map(|r| (*r).clone()).collect::<Vec<_>>(),
            &mut random,
        )
        .map_err(|_| PveError::InvalidProfile)?;
        for index in selected {
            let tree = bace_loot::materialize_create_list_tree(rows[index], &p.templates)
                .map_err(|_| PveError::InvalidProfile)?;
            let offset = items.len();
            for node in tree {
                parents.push(node.parent_index.map(|parent| parent + offset));
                items.push(node.source);
            }
        }
        if items.len() > 256 {
            return Err(PveError::Capacity);
        }
        let rare: Option<RareDecision> = if let (Some(evaluator), Some(owner)) = (&p.rare, owner) {
            if world.combatant(owner).is_some_and(|c| c.profile().player) {
                let state = characters.rare(owner).ok_or(PveError::MissingActor)?;
                let level = match world
                    .properties(owner)
                    .and_then(|p| p.get(bace_entity::PropertyFamily::Int, 25))
                {
                    Some(bace_entity::PropertyValue::Int(v)) => u32::try_from(*v)
                        .ok()
                        .filter(|v| *v > 0)
                        .ok_or(PveError::InvalidProfile)?,
                    _ => return Err(PveError::InvalidProfile),
                };
                let now = self
                    .epoch
                    .ok_or(PveError::InvalidProfile)?
                    .checked_add(tick / 30)
                    .ok_or(PveError::Overflow)?;
                Some(
                    evaluator
                        .evaluate(
                            root,
                            state,
                            RareKillContext {
                                character: owner.0,
                                player_level: level,
                                creature_level: p.creature_level,
                                lootable_monster_death: true,
                                now_unix_seconds: now,
                            },
                        )
                        .map_err(|_| PveError::InvalidProfile)?,
                )
            } else {
                None
            }
        } else {
            None
        };
        let mut generated = items
            .iter()
            .enumerate()
            .map(|(i, w)| {
                Ok(bace_loot::LootDrop {
                    template: w.weenie_id,
                    stack: stack(w)?,
                    node: format!("ace:{i}"),
                    mutations: vec![],
                })
            })
            .collect::<Result<Vec<_>, PveError>>()?;
        if let Some(award) = rare.as_ref().and_then(|r| r.award) {
            let template = p
                .templates
                .get(&award.template)
                .ok_or(PveError::InvalidProfile)?;
            items.push((**template).clone());
            generated.push(bace_loot::LootDrop {
                template: award.template,
                stack: 1,
                node: "$rare".into(),
                mutations: vec![],
            });
        }
        Ok(Some(NativeDeathLoot {
            source_parents: Some({
                parents.resize(items.len(), None);
                parents
            }),
            source_items: Some(items),
            event_id: ctx.event,
            key_version: root.key_version(),
            graph_id: p.profile_id,
            graph_revision: p.profile_revision,
            content_generation: p.content_generation,
            rare_profile_revision: p.rare_profile_revision,
            generated,
            rare,
        }))
    }
}
fn stack(item: &WeenieV1) -> Result<u32, PveError> {
    u32::try_from(
        item.properties
            .ints
            .iter()
            .find(|p| p.id == 12)
            .map_or(1, |p| p.value),
    )
    .ok()
    .filter(|v| *v > 0)
    .ok_or(PveError::InvalidProfile)
}
impl Population {
    pub(crate) fn configure_generator_loot(
        &mut self,
        root: Arc<RandomRoot>,
        epoch: u64,
    ) -> Result<(), PveError> {
        if let Some(existing) = &self.native.root {
            if !Arc::ptr_eq(existing, &root) || self.native.epoch != Some(epoch) {
                return Err(PveError::InvalidProfile);
            }
            return Ok(());
        }
        if !self.random.is_empty() {
            return Err(PveError::InvalidRandom);
        }
        self.native.root = Some(root);
        self.native.epoch = Some(epoch);
        Ok(())
    }
    pub(crate) fn ace_loot(
        &mut self,
        actor: EntityId,
        policy: AceCreatureLootPolicy,
        event: [u8; 16],
    ) -> Result<(), PveError> {
        if !self.npcs.contains_key(&actor)
            || self.native.root.is_none()
            || self.native.aces.contains_key(&actor)
            || self.native.contexts.contains_key(&actor)
            || self.pending.values().any(|p| p.proposal.victim == actor)
            || event == [0; 16]
            || policy.profile_id == 0
            || policy.profile_revision == [0; 32]
            || policy.content_generation == [0; 32]
            || policy.creature_level == 0
            || policy.rare.is_some() != policy.rare_profile_revision.is_some()
            || self.native.aces.values().any(|c| c.event == event)
            || self.native.contexts.values().any(|c| c.event == event)
            || self.native.events.values().any(|e| *e == event)
        {
            return Err(PveError::InvalidProfile);
        }
        let npc = self.npcs.get_mut(&actor).expect("validated creature owner");
        npc.combat_ai = source_combat_ai(&policy.source);
        npc.tolerance = source_tolerance(&policy.source);
        let originals = policy
            .initial_ids
            .iter()
            .copied()
            .zip(policy.initial_items.iter().cloned())
            .collect();
        self.native.aces.insert(
            actor,
            AceContext {
                policy,
                event,
                originals,
            },
        );
        Ok(())
    }
}

/// ACE Creature.SetMonsterState: passive non-attackable creatures have no combat AI.
pub(super) fn source_combat_ai(source: &WeenieV1) -> bool {
    let attackable = source
        .properties
        .bools
        .iter()
        .find(|p| p.id == 19)
        .is_none_or(|p| p.value);
    let targeting_tactic = source
        .properties
        .ints
        .iter()
        .find(|p| p.id == 68)
        .map_or(0, |p| p.value);
    attackable || targeting_tactic != 0
}
/// ACE Monster_Awareness.Tolerance reads PropertyInt.Tolerance (67) as flags.
pub(super) fn source_tolerance(source: &WeenieV1) -> u32 {
    source
        .properties
        .ints
        .iter()
        .find(|p| p.id == 67)
        .map_or(0, |p| p.value as u32)
}
