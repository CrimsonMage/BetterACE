//! Retained shutdown preflights every lane before transferring any thread handle.
use super::*;
impl GameRuntime {
    pub(in crate::game_runtime) fn shutdown_npc_workers(
        &mut self,
    ) -> Result<Vec<std::thread::JoinHandle<()>>, String> {
        if self.npc.has_pending()
            || self
                .npc
                .item_worker
                .as_ref()
                .is_some_and(|w| w.has_pending())
            || self
                .npc
                .motion_worker
                .as_ref()
                .is_some_and(|w| w.has_pending())
            || self
                .npc
                .recovery_worker
                .as_ref()
                .is_some_and(|w| w.has_pending())
            || self
                .npc
                .server_magic
                .as_ref()
                .is_some_and(|w| w.has_pending())
        {
            return Err("NPC worker shutdown requires all exact completions to drain".into());
        }
        let mut handles = Vec::with_capacity(4);
        if let Some(worker) = self.npc.item_worker.take() {
            match worker.try_shutdown() {
                Ok(handle) => handles.push(handle),
                Err(worker) => {
                    self.npc.item_worker = Some(*worker);
                    return Err("NPC item shutdown retained".into());
                }
            }
        }
        if let Some(worker) = self.npc.motion_worker.take() {
            match worker.try_shutdown() {
                Ok(handle) => handles.push(handle),
                Err(worker) => {
                    self.npc.motion_worker = Some(*worker);
                    return Err("NPC motion shutdown retained".into());
                }
            }
        }
        if let Some(worker) = self.npc.recovery_worker.take() {
            match worker.try_shutdown() {
                Ok(handle) => handles.push(handle),
                Err(worker) => {
                    self.npc.recovery_worker = Some(*worker);
                    return Err("NPC recovery shutdown retained".into());
                }
            }
        }
        if let Some(worker) = self.npc.server_magic.take() {
            match worker.try_shutdown() {
                Ok(handle) => handles.push(handle),
                Err(worker) => {
                    self.npc.server_magic = Some(*worker);
                    return Err("NPC spell shutdown retained".into());
                }
            }
        }
        Ok(handles)
    }
}
