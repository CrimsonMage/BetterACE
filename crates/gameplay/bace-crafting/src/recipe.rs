//! Immutable proposals; only the simulation owner may commit a confirmed receipt.
use crate::{
    ChanceInput, CraftError, Mutation, MutationKind, Participant, PropertyKey, PropertyValue,
    Requirement, TinkerChance, tinker_chance,
};
use bace_random::{RandomRoot, RandomStream};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct CraftItem {
    pub id: u32,
    pub owner: u32,
    pub revision: u64,
    pub stack: u32,
    pub equipped: bool,
    pub in_trade: bool,
    pub reserved: bool,
    pub times_tinkered: u32,
    pub tinker_log: Vec<u32>,
    pub properties: BTreeMap<PropertyKey, PropertyValue>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CraftContext {
    pub actor: u32,
    pub actor_revision: u64,
    pub character_random_id: [u8; 16],
    pub operation_id: [u8; 16],
    pub busy: bool,
    pub peace_mode: bool,
    pub chance: ChanceInput,
    pub properties: BTreeMap<PropertyKey, PropertyValue>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RecipeBranch {
    pub consume_source: u32,
    pub consume_target: u32,
    pub destroy_source_chance: f64,
    pub destroy_target_chance: f64,
    pub mutations: Vec<Mutation>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedRecipe {
    pub id: u32,
    pub revision: u64,
    pub requirements: Vec<Requirement>,
    pub success: RecipeBranch,
    pub failure: RecipeBranch,
    pub increment_tinker_count: bool,
    /// Source OnSuccessUse skill and positive authored difficulty.
    pub proficiency: Option<(u32, u32)>,
}
impl PreparedRecipe {
    /// Reject oversized retained recipe data before quoting or worker admission.
    pub fn validate_bounds(&self) -> Result<(), CraftError> {
        if self.requirements.len() > 256
            || self.success.mutations.len() > 256
            || self.failure.mutations.len() > 256
        {
            return Err(CraftError::Capacity);
        }
        if self.increment_tinker_count
            && self
                .success
                .mutations
                .iter()
                .chain(&self.failure.mutations)
                .any(|m| matches!(m.kind, MutationKind::Script(_)))
        {
            return Err(CraftError::InvalidState);
        }
        let values = self.requirements.iter().map(|r| &r.value).chain(
            self.success
                .mutations
                .iter()
                .chain(&self.failure.mutations)
                .filter_map(|m| match &m.kind {
                    MutationKind::Set(value) | MutationKind::Add(value) => Some(value),
                    _ => None,
                }),
        );
        let mut bytes = 0usize;
        for value in values {
            if !value.valid() {
                return Err(CraftError::InvalidState);
            }
            let size = match value {
                PropertyValue::String(text) => text.len().saturating_add(32),
                _ => 32,
            };
            bytes = bytes.checked_add(size).ok_or(CraftError::Capacity)?;
            if bytes > 65536 {
                return Err(CraftError::Capacity);
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Confirmation {
    pub(crate) context: CraftContext,
    pub(crate) source: CraftItem,
    pub(crate) target: CraftItem,
    pub(crate) recipe: PreparedRecipe,
    chance: TinkerChance,
    pub(crate) expires_at: u64,
}
impl Confirmation {
    pub fn chance(&self) -> TinkerChance {
        self.chance
    }
    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }
    pub fn operation_id(&self) -> [u8; 16] {
        self.context.operation_id
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct CraftPropertyUpdate {
    pub participant: Participant,
    pub key: PropertyKey,
    pub value: PropertyValue,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CraftProposal {
    /// Source-ordered explicit Modify* messages; script effects send UpdateObject.
    pub property_updates: Vec<CraftPropertyUpdate>,
    pub operation_id: [u8; 16],
    pub random_key_version: u32,
    pub actor: u32,
    pub expected_actor_revision: u64,
    pub actor_revision: u64,
    pub actor_properties: BTreeMap<PropertyKey, PropertyValue>,
    pub actor_before_properties: BTreeMap<PropertyKey, PropertyValue>,
    pub source_before: CraftItem,
    pub target_before: CraftItem,
    pub source_after: Option<CraftItem>,
    pub target_after: Option<CraftItem>,
    pub success: bool,
    pub destroy_source: bool,
    pub destroy_target: bool,
    pub chance: TinkerChance,
    pub imbue: bool,
}

/// Pure resolution using the same success draw as the proposal path.
pub fn roll_tinker(
    input: ChanceInput,
    root: &RandomRoot,
    character: [u8; 16],
    operation: [u8; 16],
) -> Result<(TinkerChance, bool), CraftError> {
    let chance = tinker_chance(input)?;
    let stream = root
        .crafting_stream(character, operation)
        .map_err(|_| CraftError::Random)?;
    Ok((chance, draw(&stream, b"success", chance.probability)?))
}

pub fn quote_craft(
    context: &CraftContext,
    source: &CraftItem,
    target: &CraftItem,
    recipe: &PreparedRecipe,
    now: u64,
    lifetime: u64,
) -> Result<Confirmation, CraftError> {
    validate(context, source, target, recipe)?;
    if lifetime == 0 || lifetime > 1800 {
        return Err(CraftError::InvalidState);
    }
    let expires_at = now.checked_add(lifetime).ok_or(CraftError::Overflow)?;
    Ok(Confirmation {
        context: context.clone(),
        source: source.clone(),
        target: target.clone(),
        recipe: recipe.clone(),
        chance: tinker_chance(context.chance)?,
        expires_at,
    })
}

/// Revalidate before deriving any draws. Reusing an operation deterministically
/// reconstructs its outcome; durable operation receipts reject duplicate commits.
pub fn propose_craft(
    context: &CraftContext,
    source: &CraftItem,
    target: &CraftItem,
    recipe: &PreparedRecipe,
    confirmation: &Confirmation,
    now: u64,
    root: &RandomRoot,
) -> Result<CraftProposal, CraftError> {
    if now >= confirmation.expires_at
        || context != &confirmation.context
        || source != &confirmation.source
        || target != &confirmation.target
        || recipe != &confirmation.recipe
    {
        return Err(CraftError::StaleConfirmation);
    }
    validate(context, source, target, recipe)?;
    let stream = root
        .crafting_stream(context.character_random_id, context.operation_id)
        .map_err(|_| CraftError::Random)?;
    let chance = tinker_chance(context.chance)?;
    let success = draw(&stream, b"success", chance.probability)?;
    let branch = if success {
        &recipe.success
    } else {
        &recipe.failure
    };
    let mut source_after = source.clone();
    let mut target_after = target.clone();
    let mut actor_properties = context.properties.clone();
    if context.chance.imbue {
        for id in if success { &[205, 206][..] } else { &[205][..] } {
            crate::types::mutate(
                &mut actor_properties,
                &Mutation {
                    participant: Participant::Actor,
                    key: PropertyKey {
                        kind: crate::PropertyKind::Int,
                        id: *id,
                    },
                    kind: MutationKind::Add(PropertyValue::Int(1)),
                },
                None,
            )?;
        }
    }
    let mut property_updates = Vec::new();
    for mutation in &branch.mutations {
        let copied = if let MutationKind::Copy { participant, key } = mutation.kind {
            let properties = match participant {
                Participant::Actor => &actor_properties,
                Participant::Source => &source_after.properties,
                Participant::Target => &target_after.properties,
            };
            Some(properties.get(&key).cloned().unwrap_or_else(|| {
                match key.kind {
                    crate::PropertyKind::SpellBook => PropertyValue::SpellBook(false),
                    crate::PropertyKind::Bool => PropertyValue::Bool(false),
                    crate::PropertyKind::Int => PropertyValue::Int(0),
                    crate::PropertyKind::Int64 => PropertyValue::Int64(0),
                    crate::PropertyKind::Float => PropertyValue::Float(0.0),
                    crate::PropertyKind::DataId => PropertyValue::DataId(0),
                    crate::PropertyKind::InstanceId => PropertyValue::InstanceId(0),
                    crate::PropertyKind::String => properties
                        .get(&PropertyKey {
                            kind: crate::PropertyKind::String,
                            id: 1,
                        })
                        .cloned()
                        .unwrap_or_else(|| PropertyValue::String(String::new())),
                }
            }))
        } else if let MutationKind::CopyIdentity(participant) = mutation.kind {
            Some(PropertyValue::InstanceId(match participant {
                Participant::Actor => context.actor,
                Participant::Source => source.id,
                Participant::Target => target.id,
            }))
        } else {
            None
        };
        let properties = match mutation.participant {
            Participant::Actor => &mut actor_properties,
            Participant::Source => &mut source_after.properties,
            Participant::Target => &mut target_after.properties,
        };
        crate::types::mutate(properties, mutation, copied)?;
        if !matches!(mutation.kind, MutationKind::Script(_))
            && mutation.key.kind != crate::PropertyKind::SpellBook
        {
            let value =
                properties
                    .get(&mutation.key)
                    .cloned()
                    .unwrap_or_else(|| match mutation.key.kind {
                        crate::PropertyKind::Bool => PropertyValue::Bool(false),
                        crate::PropertyKind::Int => PropertyValue::Int(0),
                        crate::PropertyKind::Int64 => PropertyValue::Int64(0),
                        crate::PropertyKind::Float => PropertyValue::Float(0.0),
                        crate::PropertyKind::String => PropertyValue::String(String::new()),
                        crate::PropertyKind::DataId => PropertyValue::DataId(0),
                        crate::PropertyKind::InstanceId => PropertyValue::InstanceId(0),
                        crate::PropertyKind::SpellBook => PropertyValue::SpellBook(false),
                    });
            property_updates.push(CraftPropertyUpdate {
                participant: mutation.participant,
                key: mutation.key,
                value,
            });
        }
        if matches!(mutation.kind, MutationKind::Script(_)) {
            if mutation.participant != Participant::Target {
                return Err(CraftError::Unsupported);
            }
            let key = PropertyKey {
                kind: crate::PropertyKind::Int,
                id: 171,
            };
            let count = match target_after.properties.get(&key) {
                Some(PropertyValue::Int(value)) => {
                    u32::try_from(*value).map_err(|_| CraftError::InvalidState)?
                }
                None => 0,
                _ => return Err(CraftError::InvalidState),
            };
            if count != target_after.times_tinkered {
                if count > 10 || target_after.tinker_log.len() >= 128 {
                    return Err(CraftError::TinkerLimit);
                }
                target_after.times_tinkered = count;
                target_after.tinker_log.push(context.chance.material);
            }
        }
    }
    if success && recipe.increment_tinker_count {
        target_after.times_tinkered = target_after
            .times_tinkered
            .checked_add(1)
            .ok_or(CraftError::Overflow)?;
        if target_after.times_tinkered > 10 || target_after.tinker_log.len() >= 128 {
            return Err(CraftError::TinkerLimit);
        }
        target_after.tinker_log.push(context.chance.material);
    }
    let destroy_source = draw(&stream, b"destroy-source", branch.destroy_source_chance)?;
    let destroy_target = draw(&stream, b"destroy-target", branch.destroy_target_chance)?;
    if destroy_source {
        source_after.stack = source_after
            .stack
            .checked_sub(branch.consume_source)
            .ok_or(CraftError::InvalidState)?;
    }
    if destroy_target {
        target_after.stack = target_after
            .stack
            .checked_sub(branch.consume_target)
            .ok_or(CraftError::InvalidState)?;
    }
    if &source_after != source
        || branch
            .mutations
            .iter()
            .any(|m| m.participant == Participant::Source)
    {
        source_after.revision = source.revision.checked_add(1).ok_or(CraftError::Overflow)?;
    }
    if &target_after != target
        || branch
            .mutations
            .iter()
            .any(|m| m.participant == Participant::Target)
    {
        target_after.revision = target.revision.checked_add(1).ok_or(CraftError::Overflow)?;
    }
    let actor_revision = if actor_properties != context.properties {
        context
            .actor_revision
            .checked_add(1)
            .ok_or(CraftError::Overflow)?
    } else {
        context.actor_revision
    };
    Ok(CraftProposal {
        property_updates,
        operation_id: context.operation_id,
        random_key_version: root.key_version(),
        actor: context.actor,
        expected_actor_revision: context.actor_revision,
        actor_revision,
        actor_properties,
        actor_before_properties: context.properties.clone(),
        source_before: source.clone(),
        target_before: target.clone(),
        source_after: (source_after.stack > 0).then_some(source_after),
        target_after: (target_after.stack > 0).then_some(target_after),
        success,
        destroy_source,
        destroy_target,
        chance,
        imbue: context.chance.imbue,
    })
}

fn draw(stream: &RandomStream, label: &[u8], probability: f64) -> Result<bool, CraftError> {
    let mut draw = stream.fork(label, 0).map_err(|_| CraftError::Random)?;
    // Exact rational caps/endpoints. Other logistic values use a 53-bit draw.
    let exact = if probability == 0.33 {
        Some((33, 100))
    } else if probability == 0.38 {
        Some((38, 100))
    } else if probability == 1.0 {
        Some((1, 1))
    } else if probability == 0.0 {
        Some((0, 1))
    } else {
        None
    };
    if let Some((n, d)) = exact {
        return draw.chance(n, d).map_err(|_| CraftError::Random);
    }
    Ok(
        (draw.below(1 << 53).map_err(|_| CraftError::Random)? as f64) / ((1_u64 << 53) as f64)
            < probability,
    )
}

fn validate(
    context: &CraftContext,
    source: &CraftItem,
    target: &CraftItem,
    recipe: &PreparedRecipe,
) -> Result<(), CraftError> {
    if context.actor == 0
        || context.actor == u32::MAX
        || context.operation_id == [0; 16]
        || context.character_random_id == [0; 16]
        || recipe.id == 0
        || recipe.revision == 0
        || context.actor_revision == 0
    {
        return Err(CraftError::InvalidState);
    }
    if context.busy
        || !context.peace_mode
        || source.reserved
        || target.reserved
        || source.in_trade
        || target.in_trade
    {
        return Err(CraftError::Busy);
    }
    if source.id == target.id || source.id == context.actor || target.id == context.actor {
        return Err(CraftError::Duplicate);
    }
    for item in [source, target] {
        if item.id == 0
            || item.id == u32::MAX
            || item.revision == 0
            || item.stack == 0
            || item.tinker_log.len() > 128
        {
            return Err(CraftError::InvalidState);
        }
        if item.owner != context.actor || item.equipped {
            return Err(CraftError::Ownership);
        }
    }
    if context.chance.times_tinkered != target.times_tinkered {
        return Err(CraftError::InvalidState);
    }
    for props in [&context.properties, &source.properties, &target.properties] {
        if props.len() > 4096 {
            return Err(CraftError::Capacity);
        }
        let bytes = props
            .values()
            .try_fold(0usize, |sum, value| {
                sum.checked_add(match value {
                    PropertyValue::String(v) => v.len() + 32,
                    _ => 32,
                })
            })
            .ok_or(CraftError::Capacity)?;
        if bytes > 65536 {
            return Err(CraftError::Capacity);
        }
        if props
            .iter()
            .any(|(k, v)| k.id == 0 || k.kind != v.kind() || !v.valid())
        {
            return Err(CraftError::InvalidState);
        }
    }
    recipe.validate_bounds()?;
    if recipe
        .success
        .mutations
        .iter()
        .chain(&recipe.failure.mutations)
        .any(|m| matches!(m.kind, MutationKind::Script(_)))
    {
        let key = PropertyKey {
            kind: crate::PropertyKind::Int,
            id: 171,
        };
        let count = match target.properties.get(&key) {
            None => 0,
            Some(PropertyValue::Int(value)) => {
                u32::try_from(*value).map_err(|_| CraftError::InvalidState)?
            }
            _ => return Err(CraftError::InvalidState),
        };
        if count != target.times_tinkered {
            return Err(CraftError::InvalidState);
        }
    }
    for branch in [&recipe.success, &recipe.failure] {
        if [branch.destroy_source_chance, branch.destroy_target_chance]
            .iter()
            .any(|p| !p.is_finite() || !(0.0..=1.0).contains(p))
        {
            return Err(CraftError::InvalidState);
        }
        if branch.consume_source > source.stack || branch.consume_target > target.stack {
            return Err(CraftError::InvalidState);
        }
    }
    for requirement in &recipe.requirements {
        let props = match requirement.participant {
            Participant::Actor => &context.properties,
            Participant::Source => &source.properties,
            Participant::Target => &target.properties,
        };
        requirement.check(props)?;
    }
    Ok(())
}
