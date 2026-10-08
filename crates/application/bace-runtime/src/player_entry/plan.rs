//! Complete bounded login publication plan. Main-pack insertion order is retained;
//! contents lists follow placement order, then equipped objects follow owner order.
use super::{
    EntryAppearanceAssets, EntryInventoryItem, EntryObjectState, PlayerAppearanceOptions,
    prepare_entry_attachments, prepare_entry_object, prepare_item_model,
    prepare_player_description, prepare_player_model, prepare_titles,
};
use bace_content::{Property, WeenieV1};
use bace_gameplay_api::{CharacterBinding, EnchantmentProjection};
use bace_replication::{
    EventSequencer, LoginPossession, LoginProjection, LoginProjectionLimits, SessionBatch,
    SessionProjectionError,
};
use bace_storage_codec::{ItemPlacementV2, PlayerSaveV6};
use bace_wire::{
    CharacterTitle, ContainerEntry, FriendsUpdate, ObjectDescription, PhysicsMovement,
    PhysicsSequences, PlayerDescription,
};
use std::collections::BTreeMap;

pub struct PlayerEntryInput<'a> {
    pub saved: &'a PlayerSaveV6,
    /// Accepted owner iteration order, shared with the description/equipment list.
    pub items: &'a [EntryInventoryItem<'a>],
    pub enchantments: &'a [EnchantmentProjection],
    pub friends: FriendsUpdate,
    pub actor_state: EntryObjectState,
    /// Canonical object sequence owners, supplied once for every possession.
    pub item_sequences: &'a BTreeMap<u32, PhysicsSequences>,
    pub missile_combat: bool,
    pub plussed: bool,
    pub assets: &'a EntryAppearanceAssets<'a>,
}
#[derive(Clone, Debug)]
pub enum PreparedEntryPossession {
    Create(Box<ObjectDescription>),
    Contents {
        container_id: u32,
        items: Vec<ContainerEntry>,
    },
}
pub struct PreparedPlayerEntry {
    pub description: PlayerDescription,
    pub titles: CharacterTitle,
    pub friends: FriendsUpdate,
    pub self_object: ObjectDescription,
    pub possessions: Vec<PreparedEntryPossession>,
}
impl PreparedPlayerEntry {
    /// Encoding and capacity admission complete before the shared event counter moves.
    pub fn project(
        &self,
        events: &mut EventSequencer,
        binding: CharacterBinding,
        limits: LoginProjectionLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        let possessions = self
            .possessions
            .iter()
            .map(|p| match p {
                PreparedEntryPossession::Create(object) => LoginPossession::Create(object),
                PreparedEntryPossession::Contents {
                    container_id,
                    items,
                } => LoginPossession::Contents {
                    container_id: *container_id,
                    items,
                },
            })
            .collect::<Vec<_>>();
        events.project_login(
            binding,
            LoginProjection {
                description: &self.description,
                titles: &self.titles,
                friends: &self.friends,
                self_object: &self.self_object,
                possessions: &possessions,
            },
            limits,
        )
    }
}
pub fn prepare_player_entry(input: PlayerEntryInput<'_>) -> Result<PreparedPlayerEntry, String> {
    let PlayerEntryInput {
        saved,
        items,
        enchantments,
        friends,
        mut actor_state,
        item_sequences,
        missile_combat,
        plussed,
        assets,
    } = input;
    let description = prepare_player_description(saved, items, enchantments, plussed)?;
    let titles = prepare_titles(saved)?;
    let actor = saved.player.entity.object_id;
    if !actor_state.is_player
        || !actor_state.is_creature
        || actor_state.position.is_none()
        || actor_state.parent.is_some()
        || item_sequences.len() != items.len()
    {
        return Err("entry owner state/sequence closure mismatch".into());
    }
    if friends.kind != bace_wire::FriendsUpdateKind::Full {
        return Err("entry requires full accepted friends snapshot".into());
    }
    let mut normalized = BTreeMap::new();
    let mut children: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    let mut equipped = vec![];
    for (index, item) in items.iter().enumerate() {
        let id = item.entity.object_id;
        if !item_sequences.contains_key(&id) {
            return Err("entry item sequence owner missing".into());
        }
        let ItemPlacementV2::Contained {
            container,
            slot,
            equipped: location,
            ..
        } = *item.placement
        else {
            return Err("entry item not contained".into());
        };
        let mut source = item.entity.state.clone();
        set(
            &mut source.properties.instance_ids,
            2,
            (location == 0).then_some(container),
        );
        set(
            &mut source.properties.instance_ids,
            3,
            (location != 0).then_some(container),
        );
        set(
            &mut source.properties.ints,
            10,
            (location != 0).then_some(location as i32),
        );
        set(
            &mut source.properties.ints,
            53,
            Some(i32::try_from(slot).map_err(|_| "entry slot overflow")?),
        );
        if location != 0 {
            equipped.push((id, index, location));
        } else {
            children.entry(container).or_default().push(index);
        }
        normalized.insert(id, source);
    }
    let equipment = equipped
        .iter()
        .map(|(id, _, loc)| (*id, &normalized[id], *loc))
        .collect::<Vec<_>>();
    let (attachments, item_attachments) =
        prepare_entry_attachments(actor, &equipment, missile_combat)?;
    actor_state.children = attachments;
    let actor_position = actor_state.position;
    let model = prepare_player_model(
        &saved.player.entity.state,
        &equipment.iter().map(|(_, s, _)| *s).collect::<Vec<_>>(),
        PlayerAppearanceOptions {
            show_helm: saved.player.metadata.options2 & 0x00100000 != 0,
            show_cloak: saved.player.metadata.options2 & 0x00800000 != 0,
            default_hair_texture: saved.player.metadata.default_hair_texture,
            hair_texture: saved.player.metadata.hair_texture,
        },
        assets,
    )?;
    let mut player_source = saved.player.entity.state.clone();
    // Source SendSelf removes placement before its ObjectCreate.
    set(&mut player_source.properties.ints, 65, None);
    if plussed {
        let name = player_source
            .properties
            .strings
            .iter_mut()
            .find(|p| p.id == 1)
            .ok_or("plussed entry name missing")?;
        name.value = format!("+{}", name.value);
    }
    let self_object = prepare_entry_object(actor, &player_source, model, actor_state)?;
    let mut objects = BTreeMap::new();
    for item in items {
        let id = item.entity.object_id;
        let source = &normalized[&id];
        let attachment = item_attachments.iter().find(|v| v.item == id);
        let state = EntryObjectState {
            is_player: false,
            is_creature: false,
            physics_state: source
                .properties
                .ints
                .iter()
                .find(|p| p.id == 93)
                .map_or(0x00400c08, |p| p.value as u32),
            position: attachment
                .filter(|v| v.parent.is_some())
                .and(actor_position),
            movement: Some(PhysicsMovement::AnimationFrame(
                attachment.map_or(101, |v| v.placement),
            )),
            parent: attachment.and_then(|v| v.parent),
            children: vec![],
            velocity: [0.; 3],
            acceleration: [0.; 3],
            omega: [0.; 3],
            sequences: item_sequences[&id],
            admin_vision: false,
            change_no_draw: false,
            cloak_status: 0,
        };
        objects.insert(
            id,
            prepare_entry_object(id, source, prepare_item_model(source, assets)?, state)?,
        );
    }
    let mut possessions = vec![];
    append_container(
        actor,
        0,
        items,
        &normalized,
        &children,
        &mut objects,
        &mut possessions,
    )?;
    for (id, _, _) in equipped {
        let object = objects
            .remove(&id)
            .ok_or("entry equipped object repeated")?;
        possessions.push(PreparedEntryPossession::Create(Box::new(object)));
    }
    if !objects.is_empty() {
        return Err("entry inventory publication closure incomplete".into());
    }
    Ok(PreparedPlayerEntry {
        description,
        titles,
        friends,
        self_object,
        possessions,
    })
}
fn append_container(
    parent: u32,
    depth: usize,
    items: &[EntryInventoryItem<'_>],
    sources: &BTreeMap<u32, WeenieV1>,
    children: &BTreeMap<u32, Vec<usize>>,
    objects: &mut BTreeMap<u32, ObjectDescription>,
    output: &mut Vec<PreparedEntryPossession>,
) -> Result<(), String> {
    if depth > 64 {
        return Err("entry inventory publication depth".into());
    }
    for index in children.get(&parent).into_iter().flatten() {
        let item = &items[*index];
        let id = item.entity.object_id;
        let object = objects
            .remove(&id)
            .ok_or("entry inventory object repeated")?;
        output.push(PreparedEntryPossession::Create(Box::new(object)));
        let source = &sources[&id];
        let container = matches!(source.weenie_type, 14 | 20 | 21 | 56 | 57);
        if !container && children.contains_key(&id) {
            return Err("entry child attached to non-container".into());
        }
        if container {
            let mut contents = children
                .get(&id)
                .into_iter()
                .flatten()
                .map(|i| {
                    let child = &items[*i];
                    let ItemPlacementV2::Contained { slot, .. } = *child.placement else {
                        unreachable!("validated placement")
                    };
                    let state = &sources[&child.entity.object_id];
                    (
                        slot,
                        ContainerEntry {
                            object_id: child.entity.object_id,
                            container_type: if state.weenie_type == 21 {
                                1
                            } else if state.properties.bools.iter().any(|p| p.id == 81 && p.value) {
                                2
                            } else {
                                0
                            },
                        },
                    )
                })
                .collect::<Vec<_>>();
            contents.sort_by_key(|(slot, _)| *slot);
            output.push(PreparedEntryPossession::Contents {
                container_id: id,
                items: contents.into_iter().map(|(_, v)| v).collect(),
            });
            append_container(id, depth + 1, items, sources, children, objects, output)?;
        }
    }
    Ok(())
}
fn set<T>(properties: &mut Vec<Property<T>>, id: u32, value: Option<T>) {
    if let Some(value) = value {
        if let Some(p) = properties.iter_mut().find(|p| p.id == id) {
            p.value = value;
        } else {
            properties.push(Property { id, value });
            properties.sort_by_key(|p| p.id);
        }
    } else {
        properties.retain(|p| p.id != id);
    }
}
