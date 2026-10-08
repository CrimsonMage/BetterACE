use crate::{PgStore, StoreError};
impl PgStore {
    /// Bind a public fingerprint to a version. Existing streams cannot silently
    /// switch keys after local key loss. The master key is never accepted here.
    pub async fn bind_random_key(&self, version: u32, sha256: [u8; 32]) -> Result<(), StoreError> {
        if version == 0 {
            return Err(StoreError::Invalid("random key version"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO random_key_fingerprints(version,sha256) VALUES($1,$2) ON CONFLICT(version) DO NOTHING").bind(i64::from(version)).bind(sha256.as_slice()).execute(&mut *tx).await?;
        let saved: Vec<u8> =
            sqlx::query_scalar("SELECT sha256 FROM random_key_fingerprints WHERE version=$1")
                .bind(i64::from(version))
                .fetch_one(&mut *tx)
                .await?;
        if saved != sha256 {
            return Err(StoreError::Invalid(
                "random key version fingerprint mismatch",
            ));
        }
        tx.commit().await.map_err(crate::store::commit_error)
    }
}
