//! Read/allocation adapter port; production SQL remains in bace-db-postgres.
use bace_persistence::StoredAggregate;
use std::future::Future;
pub trait GeneratorRepository: Send + Sync + 'static {
    fn allocate(&self, count: u16) -> impl Future<Output = Result<Vec<u32>, String>> + Send;
    fn load(&self, id: u32)
    -> impl Future<Output = Result<Option<StoredAggregate>, String>> + Send;
}
impl GeneratorRepository for bace_db_postgres::PgStore {
    async fn allocate(&self, count: u16) -> Result<Vec<u32>, String> {
        self.allocate_dynamic_ids(count)
            .await
            .map_err(|e| e.to_string())
    }
    async fn load(&self, id: u32) -> Result<Option<StoredAggregate>, String> {
        bace_db_postgres::PgStore::load(self, id)
            .await
            .map_err(|e| e.to_string())
    }
}
