use crate::backend::common_models::Value;
use crate::driver::dialect::Dialect;
use crate::driver::model::DatabaseBackend;
use async_trait::async_trait;
use std::collections::HashMap;

pub struct BackendExecutor<'a> {
    backend: &'a dyn DatabaseBackend,
}

impl<'a> BackendExecutor<'a> {
    pub fn new(backend: &'a dyn DatabaseBackend) -> Self {
        Self { backend }
    }
}

#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(&mut self, sql: &str, params: &[Value])
    -> crate::backend::errors::Result<u64>;
    async fn fetch_one(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<HashMap<String, Value>>;
    async fn fetch_all(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<Vec<HashMap<String, Value>>>;
    async fn fetch_optional(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<Option<HashMap<String, Value>>>;
    fn dialect(&self) -> Dialect;
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
