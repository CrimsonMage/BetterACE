//! Cold identity evidence is joined only after the original action is authorized.
use super::*;
use bace_gameplay_api::social::SocialIdentity;
impl Kernel {
    pub fn request_social_resolved(
        &mut self,
        context: ActionContext,
        request: SocialRequest,
        identity: Option<SocialIdentity>,
    ) -> Result<(), E> {
        if self.social.outcomes.len() >= self.social.capacity
            || self.social.events.len().saturating_add(4) > self.social.capacity
        {
            return Err(E::Capacity);
        }
        let authorization = self.authorize_social(context);
        let retryable = authorization == Err(E::Busy);
        let result = authorization.and_then(|()| {
            if identity.is_none() {
                let text = match &request {
                    SocialRequest::AddFriend(_) => "That character does not exist".into(),
                    SocialRequest::AccountSquelch { name, .. } => format!("{name} not found."),
                    SocialRequest::CharacterSquelch { target, name, .. } => {
                        if target.0 != 0 {
                            "Couldn't find player to squelch.".into()
                        } else {
                            format!("{name} not found.")
                        }
                    }
                    _ => return Err(E::Invalid),
                };
                self.social.events.push_back(SocialEvent::System {
                    recipient: context.actor,
                    text,
                    chat_type: 0,
                });
                return Err(E::Missing);
            }
            let valid = match (&request, &identity) {
                (
                    SocialRequest::AddFriend(name) | SocialRequest::AccountSquelch { name, .. },
                    Some(identity),
                ) => name.trim_start_matches('+').to_lowercase() == identity.name.to_lowercase(),
                (SocialRequest::CharacterSquelch { target, name, .. }, Some(identity)) => {
                    if target.0 != 0 {
                        *target == identity.character
                    } else {
                        name.trim_start_matches('+').to_lowercase() == identity.name.to_lowercase()
                    }
                }
                (
                    SocialRequest::AddFriend(_)
                    | SocialRequest::AccountSquelch { .. }
                    | SocialRequest::CharacterSquelch { .. },
                    None,
                ) => return Err(E::Missing),
                _ => false,
            };
            if !valid {
                return Err(E::Stale);
            }
            let identity = identity.ok_or(E::Missing)?;
            self.social.directory.cache_identity(identity)?;
            self.apply_social(context.actor, request)
        });
        self.social.outcomes.push_back(SocialOutcome {
            context,
            result,
            retryable,
        });
        result
    }
}
