use crate::backend::common_models::Value;
use crate::core::executor::Executor;
use crate::core::transaction::DatabaseTransaction;
use crate::driver::dialect::Dialect;
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait DatabaseBackend: Send + Sync {
    async fn execute(&self, sql: &str, params: &[Value]) -> crate::backend::errors::Result<u64>;
    async fn fetch_one(
        &self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<HashMap<String, Value>>;
    async fn fetch_all(
        &self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<Vec<HashMap<String, Value>>>;
    async fn fetch_optional(
        &self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<Option<HashMap<String, Value>>>;
    async fn begin_transaction(
        &self,
    ) -> crate::backend::errors::Result<Box<dyn DatabaseTransaction>>;
    fn dialect(&self) -> Dialect;
    async fn ping(&self) -> crate::backend::errors::Result<()>;
}

struct BackendExecutor<'a> {
    backend: &'a dyn DatabaseBackend,
}

#[async_trait]
impl<'a> Executor for BackendExecutor<'a> {
    async fn execute(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<u64> {
        self.backend.execute(sql, params).await
    }

    async fn fetch_one(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<HashMap<String, Value>> {
        self.backend.fetch_one(sql, params).await
    }

    async fn fetch_all(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<Vec<HashMap<String, Value>>> {
        self.backend.fetch_all(sql, params).await
    }

    async fn fetch_optional(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<Option<HashMap<String, Value>>> {
        self.backend.fetch_optional(sql, params).await
    }

    fn dialect(&self) -> Dialect {
        self.backend.dialect()
    }
}
