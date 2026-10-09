//! Startup-only randomness admission. Never initialize or reseed a missing key
//! during a game restart: provisioning is a separate fresh-install operation.
use bace_db_postgres::PgStore;
use bace_random::RandomRoot;
use std::{path::Path, sync::Arc};
pub struct BoundGameplayRandom {
    root: Arc<RandomRoot>,
}
impl BoundGameplayRandom {
    pub fn root(&self) -> Arc<RandomRoot> {
        self.root.clone()
    }
    /// Startup composition only. An error aborts admission; retain/recover the
    /// caller's kernel rather than starting a partially configured world.
    pub fn configure_kernel(
        &self,
        kernel: &mut bace_simulation::Kernel,
        execution_epoch: u64,
        unix_seconds: u64,
    ) -> Result<(), String> {
        if execution_epoch == 0 || unix_seconds > i64::MAX as u64 || kernel.ticks() != 0 {
            return Err("randomness composition requires a fresh owner and durable epoch".into());
        }
        kernel
            .configure_native_loot(self.root(), unix_seconds)
            .map_err(|e| format!("native randomness: {e:?}"))?;
        kernel
            .configure_crafting_random(self.root())
            .map_err(|e| format!("crafting randomness: {e:?}"))?;
        kernel
            .configure_magic_random_shared(self.root())
            .map_err(|e| format!("magic randomness: {e:?}"))?;
        kernel
            .configure_magic_epoch(execution_epoch)
            .map_err(|e| format!("magic execution epoch: {e:?}"))?;
        kernel
            .configure_combat_random_shared(self.root(), execution_epoch)
            .map_err(|e| format!("combat randomness: {e:?}"))?;
        kernel
            .configure_recall_random(self.root(), execution_epoch)
            .map_err(|e| format!("recall randomness: {e:?}"))?;
        kernel
            .configure_social_random(self.root())
            .map_err(|e| format!("social randomness: {e:?}"))?;
        kernel
            .configure_player_death_random(self.root(), execution_epoch)
            .map_err(|e| format!("death randomness: {e:?}"))?;
        kernel
            .configure_npc_services_without_catalogs(
                self.root(),
                u32::try_from(execution_epoch).map_err(|_| "NPC execution epoch overflow")?,
            )
            .map_err(|e| format!("NPC randomness: {e:?}"))?;
        Ok(())
    }
    pub fn version(&self) -> u32 {
        self.root.key_version()
    }
}
pub async fn load_and_bind(store: &PgStore, path: &Path) -> Result<BoundGameplayRandom, String> {
    let path = path.to_owned();
    let key = tokio::task::spawn_blocking(move || crate::random_keys::load_random_key(&path))
        .await
        .map_err(|e| format!("random key worker failed: {e}"))??;
    store
        .bind_random_key(key.root.key_version(), key.fingerprint)
        .await
        .map_err(|e| e.to_string())?;
    Ok(BoundGameplayRandom {
        root: Arc::new(key.root),
    })
}
