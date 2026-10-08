//! Pinned AdminCommands.HandleGamecast/HandleGameCastEmote. Local aliases
//! deliberately reach the same single-server audience at the official pin.
use super::*;
use bace_gameplay_api::staff::{StaffError, StaffEvent};
impl Kernel {
    pub fn staff_broadcast(
        &mut self,
        context: ActionContext,
        token: u64,
        text: String,
        emote: bool,
        local: bool,
        sudo: bool,
    ) -> Result<(), StaffError> {
        if token == 0 || text.len() > 4096 {
            return Err(StaffError::Invalid);
        }
        if !self.staff.room(2) {
            return Err(StaffError::Capacity);
        }
        let sender = self
            .social
            .directory
            .presence(context.actor)
            .filter(|p| p.online)
            .ok_or(StaffError::MissingTarget)?
            .identity
            .name
            .clone();
        let text = if emote {
            text.replace("\\n", "\n")
        } else {
            format!("Broadcast from {sender}> {text}")
        };
        if text.len() > 4096 {
            return Err(StaffError::Capacity);
        }
        let recipients = self
            .social
            .directory
            .online()
            .take(4097)
            .map(|actor| {
                self.staff
                    .registration(actor)
                    .map(|r| r.binding)
                    .ok_or(StaffError::MissingTarget)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if recipients.len() > 4096 {
            return Err(StaffError::Capacity);
        }
        self.authorize_staff(context, if emote || local { 4 } else { 3 }, sudo)?;
        self.staff.push(StaffEvent::Broadcast {
            token,
            context,
            sender,
            recipients,
            text,
        });
        Ok(())
    }
}
