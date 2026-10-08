//! Cold script definitions cross the same owner boundary as the bodies that use
//! them. No actor can tick between the spawn and exact dormant registration.
use super::*;
use crate::{
    GeneratorAction, GeneratorCommand, GeneratorCommandOutcome, GeneratorServiceError as G,
    PreparedNpcScriptSource,
};
impl Kernel {
    pub(super) fn admit_generator_script_batch(
        &mut self,
        correlation: u64,
        sources: Vec<(EntityId, PreparedNpcScriptSource)>,
        action: GeneratorAction,
    ) -> GeneratorCommandOutcome {
        let reject = |error| GeneratorCommandOutcome {
            correlation,
            result: Err(error),
            admission: None,
            request: None,
        };
        let Some(key) = action.spawn_key() else {
            return reject(G::Invalid);
        };
        let Some(request) = self
            .generators
            .requests
            .get(&key)
            .filter(|_| self.generators.submitted.contains(&key))
        else {
            return reject(G::Stale);
        };
        if sources.iter().any(|(id, _)| !request.entities.contains(id)) {
            return reject(G::Invalid);
        }
        let mut body_ids = std::collections::BTreeSet::new();
        let mut scripts: std::collections::BTreeMap<_, _> = sources
            .iter()
            .map(|(id, source)| (*id, source.clone()))
            .collect();
        let mut add_embedded = |actor, embedded: Option<PreparedNpcScriptSource>| {
            body_ids.insert(actor);
            if let Some(embedded) = embedded {
                if scripts
                    .get(&actor)
                    .is_some_and(|supplied| supplied.identity != embedded.identity)
                {
                    return false;
                }
                scripts.entry(actor).or_insert(embedded);
            }
            true
        };
        match &action {
            GeneratorAction::AdmitCreature { .. } => {
                let Some(&actor) = request.entities.first() else {
                    return reject(G::Invalid);
                };
                let embedded = self
                    .generators
                    .templates
                    .get(&(
                        key.generator.content_revision,
                        request.intent.profile.weenie_class_id,
                    ))
                    .and_then(|t| t.script.clone());
                if !add_embedded(actor, embedded) {
                    return reject(G::Stale);
                }
            }
            GeneratorAction::AdmitMixedTrees { roots, .. } => {
                for root in roots {
                    let embedded = match root {
                        crate::PreparedMixedGeneratorRoot::Creature { template, .. } => self
                            .generators
                            .templates
                            .get(&(key.generator.content_revision, *template))
                            .and_then(|t| t.script.clone()),
                        _ => None,
                    };
                    if !add_embedded(root.entity(), embedded) {
                        return reject(G::Stale);
                    }
                }
            }
            GeneratorAction::AdmitItemTrees {
                roots,
                shapes: Some(_),
                ..
            } => body_ids.extend(roots.iter().copied()),
            GeneratorAction::AdmitItems {
                items,
                shapes: Some(_),
                ..
            } => body_ids.extend(items.iter().map(|i| i.id)),
            _ => return reject(G::Invalid),
        }
        if sources.iter().any(|(id, _)| !body_ids.contains(id)) {
            return reject(G::Invalid);
        }
        if self
            .preflight_npc_scripts(scripts.iter().map(|(id, source)| (*id, source)))
            .is_err()
        {
            return reject(G::Capacity);
        }
        let outcome = self.handle_generator_command(GeneratorCommand {
            correlation,
            action,
        });
        if outcome.result.is_ok() {
            for (actor, source) in sources {
                if self.world.body(actor).is_ok()
                    && self.npcs.bind_admitted(actor, source.identity).is_err()
                {
                    self.admit_npc_script(actor, source)
                        .expect("aggregate script capacity and identity preflight");
                }
            }
        }
        outcome
    }
}
