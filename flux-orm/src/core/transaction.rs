use std::collections::HashMap;
use async_trait::async_trait;
use crate::backend::common_models::Value;
use crate::core::executor::Executor;
use crate::driver::dialect::Dialect;

#[async_trait]
pub trait DatabaseTransaction: Send + Sync {
    async fn execute(&mut self, sql: &str, params: &[Value]) -> crate::backend::errors::Result<u64>;
    async fn fetch_one(&mut self, sql: &str, params: &[Value]) -> crate::backend::errors::Result<HashMap<String, Value>>;
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
    async fn commit(self: Box<Self>) -> crate::backend::errors::Result<()>;
    async fn rollback(self: Box<Self>) -> crate::backend::errors::Result<()>;
    fn dialect(&self) -> Dialect;
}

pub struct TransactionExecutor<'a> {
    tx: &'a mut Box<dyn DatabaseTransaction>,
}

impl <'a> TransactionExecutor<'a> {
    pub fn new(tx: &'a mut Box<dyn DatabaseTransaction>) -> Self {
        Self {tx}
    }
}

#[async_trait]
impl<'a> Executor for TransactionExecutor<'a> {
    async fn execute(&mut self, sql: &str, params: &[Value]) -> crate::backend::errors::Result<u64> {
        self.tx.execute(sql, params).await
    }

    async fn fetch_one(&mut self, sql: &str, params: &[Value]) -> crate::backend::errors::Result<HashMap<String, Value>> {
        self.tx.fetch_one(sql, params).await
    }

    async fn fetch_all(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<Vec<HashMap<String, Value>>> {
        self.tx.fetch_all(sql, params).await
    }

    async fn fetch_optional(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<Option<HashMap<String, Value>>> {
        self.tx.fetch_optional(sql, params).await
    }

    fn dialect(&self) -> Dialect {
        self.tx.dialect()
    }
}