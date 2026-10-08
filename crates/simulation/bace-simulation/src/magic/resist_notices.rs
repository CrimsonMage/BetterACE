//! GDLE resistance text freezes accepted names and exact private recipients at
//! the effect decision, before later renames, retirement or session replacement.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MagicResistNotice {
    pub source: Option<bace_gameplay_api::CharacterBinding>,
    pub target: Option<bace_gameplay_api::CharacterBinding>,
    pub source_name: Arc<str>,
    pub target_name: Arc<str>,
}
pub(super) fn capture(
    world: &World,
    source: EntityId,
    target: EntityId,
    observers: Option<(&crate::characters::Characters, u64)>,
) -> Result<Arc<MagicResistNotice>, CastRejection> {
    let name = |actor| match world
        .properties(actor)
        .and_then(|p| p.get(bace_entity::PropertyFamily::String, 1))
    {
        Some(bace_entity::PropertyValue::String(value)) if value.len() <= 1024 => {
            Ok(Arc::from(value.as_str()))
        }
        None => Ok(Arc::from("Unknown")),
        _ => Err(CastRejection::InvalidState),
    };
    Ok(Arc::new(MagicResistNotice {
        source: observers.and_then(|(characters, _)| characters.entered_binding(source)),
        target: observers.and_then(|(characters, _)| characters.entered_binding(target)),
        source_name: name(source)?,
        target_name: name(target)?,
    }))
}
impl Magic {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn target_rejected(
        &mut self,
        actor: EntityId,
        target: EntityId,
        spell: u32,
        reason: CastRejection,
        world: &World,
        observers: Option<(&crate::characters::Characters, u64)>,
    ) {
        let (reason, notice) = if reason == CastRejection::Resisted {
            match capture(world, actor, target, observers) {
                Ok(notice) => (reason, Some(notice)),
                Err(error) => (error, None),
            }
        } else {
            (reason, None)
        };
        self.events.push_back(MagicEvent::TargetRejected {
            actor,
            target,
            spell,
            reason,
            notice,
        });
    }
}
