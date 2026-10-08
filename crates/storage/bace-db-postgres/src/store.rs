use sqlx::{PgPool, postgres::PgPoolOptions};
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("commit acknowledgment failed; resolve durable state before retrying: {0}")]
    CommitUncertain(#[source] sqlx::Error),
    #[error("mapped generation parent does not match current accepted layout")]
    GenerationConflict,
    #[error("character ownership epoch or state mismatch")]
    OwnershipConflict,
    #[error("revision conflict for object {0}")]
    Conflict(u32),
    #[error("invalid input: {0}")]
    Invalid(&'static str),
    #[error("publication does not exist, has already been decided, or is out of order")]
    PublicationOrder,
    #[error("publication {0} exceeds bounded mapped worker capacity")]
    PublicationTooLarge(i64),
    #[error("operation ID was reused with a different request")]
    OperationMismatch,
}
#[derive(Clone, Debug)]
pub struct PgStore {
    pub(crate) pool: PgPool,
}
impl PgStore {
    pub async fn connect(url: &str, max_connections: u32) -> Result<Self, StoreError> {
        if max_connections == 0 {
            return Err(StoreError::Invalid("pool must be bounded and nonzero"));
        }
        Ok(Self {
            pool: PgPoolOptions::new()
                .max_connections(max_connections)
                .connect(url)
                .await?,
        })
    }
    /// Dedicated one-connection writer pool. Server deadlines outlive cancellation of Rust futures.
    pub async fn connect_writer(
        url: &str,
        operation_timeout: std::time::Duration,
    ) -> Result<Self, StoreError> {
        Self::connect_bounded(url, 1, operation_timeout).await
    }
    /// Pool with server deadlines that remain effective after client future cancellation.
    pub async fn connect_bounded(
        url: &str,
        max_connections: u32,
        operation_timeout: std::time::Duration,
    ) -> Result<Self, StoreError> {
        if max_connections == 0 {
            return Err(StoreError::Invalid("pool must be bounded and nonzero"));
        }
        if operation_timeout < std::time::Duration::from_millis(10)
            || operation_timeout > std::time::Duration::from_secs(60)
        {
            return Err(StoreError::Invalid(
                "operation timeout must be between 10ms and 60s",
            ));
        }
        let statement_ms = (operation_timeout.as_millis() * 4 / 5).max(1);
        let lock_ms = (statement_ms / 2).max(1);
        let idle_ms = operation_timeout.as_millis();
        let pool=PgPoolOptions::new().max_connections(max_connections).acquire_timeout(operation_timeout)
            .after_connect(move |connection,_|Box::pin(async move {
                sqlx::query("SELECT set_config('statement_timeout',$1,false),set_config('lock_timeout',$2,false),set_config('idle_in_transaction_session_timeout',$3,false)")
                    .bind(format!("{statement_ms}ms")).bind(format!("{lock_ms}ms")).bind(format!("{idle_ms}ms"))
                    .execute(connection).await?;
                Ok(())
            })).connect(url).await?;
        Ok(Self { pool })
    }
    pub async fn migrate(&self) -> Result<(), StoreError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

pub(crate) fn commit_error(error: sqlx::Error) -> StoreError {
    match error {
        // Server-reported COMMIT failure (for example a deferred constraint) is definite.
        sqlx::Error::Database(_) => StoreError::Sql(error),
        other => StoreError::CommitUncertain(other),
    }
}
