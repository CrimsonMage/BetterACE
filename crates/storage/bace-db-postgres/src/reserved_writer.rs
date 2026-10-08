//! Save capacity shares the already admitted database identity, not a later
//! environment-variable read. The writer has its own one-connection pool.
use crate::{PgStore, StoreError};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
impl PgStore {
    pub async fn reserved_writer(&self, timeout: Duration) -> Result<Self, StoreError> {
        if !(Duration::from_millis(10)..=Duration::from_secs(60)).contains(&timeout) {
            return Err(StoreError::Invalid(
                "writer timeout must be between 10ms and 60s",
            ));
        }
        let options = (*self.pool.connect_options()).clone();
        let statement_ms = (timeout.as_millis() * 4 / 5).max(1);
        let lock_ms = (statement_ms / 2).max(1);
        let idle_ms = timeout.as_millis();
        let pool=PgPoolOptions::new().max_connections(1).acquire_timeout(timeout)
            .after_connect(move |connection,_|Box::pin(async move {
                sqlx::query("SELECT set_config('statement_timeout',$1,false),set_config('lock_timeout',$2,false),set_config('idle_in_transaction_session_timeout',$3,false)")
                    .bind(format!("{statement_ms}ms")).bind(format!("{lock_ms}ms")).bind(format!("{idle_ms}ms"))
                    .execute(connection).await?;
                Ok(())
            })).connect_with(options).await?;
        Ok(Self { pool })
    }
}
