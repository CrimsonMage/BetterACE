//! A human confirmation may outlive unrelated UI, age or enchantment revisions.
//! Rebase only actor state which the quoted recipe/chance does not read.
use crate::{
    Confirmation, CraftContext, CraftError, CraftItem, CraftProposal, MutationKind, Participant,
    PreparedRecipe, PropertyKey, PropertyKind, propose_craft,
};
use bace_random::RandomRoot;

/// Retains original expiry and random identity. Source/target and recipe remain
/// exact; chance and every actor quality read by requirements/copies/arithmetic
/// must remain unchanged. No draw is taken while validating the rebase.
pub fn propose_confirmed_craft(
    context: &CraftContext,
    source: &CraftItem,
    target: &CraftItem,
    recipe: &PreparedRecipe,
    quote: &Confirmation,
    now: u64,
    root: &RandomRoot,
) -> Result<CraftProposal, CraftError> {
    let old = &quote.context;
    if context.actor != old.actor
        || context.character_random_id != old.character_random_id
        || context.operation_id != old.operation_id
        || context.chance != old.chance
        || context.actor_revision < old.actor_revision
        || source != &quote.source
        || target != &quote.target
        || recipe != &quote.recipe
        || now >= quote.expires_at
    {
        return Err(CraftError::StaleConfirmation);
    }
    let same = |key: &PropertyKey| context.properties.get(key) == old.properties.get(key);
    for requirement in &recipe.requirements {
        if requirement.participant == Participant::Actor && !same(&requirement.key) {
            return Err(CraftError::StaleConfirmation);
        }
    }
    for mutation in recipe
        .success
        .mutations
        .iter()
        .chain(&recipe.failure.mutations)
    {
        if mutation.participant == Participant::Actor && !same(&mutation.key) {
            return Err(CraftError::StaleConfirmation);
        }
        if let MutationKind::Copy {
            participant: Participant::Actor,
            key,
        } = mutation.kind
            && (!same(&key)
                || key.kind == PropertyKind::String
                    && !same(&PropertyKey {
                        kind: PropertyKind::String,
                        id: 1,
                    }))
        {
            return Err(CraftError::StaleConfirmation);
        }
    }
    let mut rebased = quote.clone();
    rebased.context = context.clone();
    propose_craft(context, source, target, recipe, &rebased, now, root)
}
