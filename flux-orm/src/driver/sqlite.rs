use crate::backend::bind_param::bind_params_sqlite;
use crate::backend::common_models::Value;
use crate::backend::errors::FluxError;
use crate::backend::row_mapper::row_to_map_sqlite;
use crate::core::transaction::DatabaseTransaction;
use crate::driver::dialect::Dialect;
use crate::driver::model::DatabaseBackend;
use async_trait::async_trait;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use sqlx::Transaction as SqlxTransaction;
use std::collections::HashMap;

pub struct SqliteBackend {
    pool: SqlitePool,
}

impl SqliteBackend {
    pub async fn new(database_url: &str) -> crate::backend::errors::Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map_err(|e| FluxError::Pool(format!("SQLite connection failed: {}", e)))?;

        Ok(Self { pool })
    }

    pub async fn with_pool_config(
        database_url: &str,
        max_connections: u32,
        min_connections: u32,
    ) -> crate::backend::errors::Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .connect(database_url)
            .await
            .map_err(|e| FluxError::Pool(format!("SQLite connection failed: {}", e)))?;

        Ok(Self { pool })
    }

    fn bind_params<'q>(
        query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
        params: &'q [Value],
    ) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
        bind_params_sqlite(query, params)
    }
}

#[async_trait]
impl DatabaseBackend for SqliteBackend {
    async fn execute(&self, sql: &str, params: &[Value]) -> crate::backend::errors::Result<u64> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let result = query.execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    async fn fetch_one(&self, sql: &str, params: &[Value]) -> crate::backend::errors::Result<HashMap<String, Value>> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let row = query.fetch_one(&self.pool).await?;
        row_to_map_sqlite(&row)
    }

    async fn fetch_all(&self, sql: &str, params: &[Value]) -> crate::backend::errors::Result<Vec<HashMap<String, Value>>> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let rows = query.fetch_all(&self.pool).await?;
        rows.iter().map(row_to_map_sqlite).collect()
    }

    async fn fetch_optional(
        &self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<Option<HashMap<String, Value>>> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let row = query.fetch_optional(&self.pool).await?;
        row.map(|r| row_to_map_sqlite(&r)).transpose()
    }

    async fn begin_transaction(&self) -> crate::backend::errors::Result<Box<dyn DatabaseTransaction>> {
        let tx = self.pool.begin().await?;
        Ok(Box::new(SqliteTransactionWrapper {
            tx: Some(tx),
            dialect: Dialect::SQLite,
        }))
    }

    fn dialect(&self) -> Dialect {
        Dialect::SQLite
    }

    async fn ping(&self) -> crate::backend::errors::Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

struct SqliteTransactionWrapper {
    tx: Option<SqlxTransaction<'static, sqlx::Sqlite>>,
    dialect: Dialect,
}

#[async_trait]
impl DatabaseTransaction for SqliteTransactionWrapper {
    async fn execute(&mut self, sql: &str, params: &[Value]) -> crate::backend::errors::Result<u64> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = SqliteBackend::bind_params(query, params);
        let result = query.execute(&mut **tx).await?;
        Ok(result.rows_affected())
    }

    async fn fetch_one(&mut self, sql: &str, params: &[Value]) -> crate::backend::errors::Result<HashMap<String, Value>> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = SqliteBackend::bind_params(query, params);
        let row = query.fetch_one(&mut **tx).await?;
        row_to_map_sqlite(&row)
    }

    async fn fetch_all(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<Vec<HashMap<String, Value>>> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = SqliteBackend::bind_params(query, params);
        let rows = query.fetch_all(&mut **tx).await?;
        rows.iter().map(row_to_map_sqlite).collect()
    }

    async fn fetch_optional(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> crate::backend::errors::Result<Option<HashMap<String, Value>>> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = SqliteBackend::bind_params(query, params);
        let row = query.fetch_optional(&mut **tx).await?;
        row.map(|r| row_to_map_sqlite(&r)).transpose()
    }

    async fn commit(mut self: Box<Self>) -> crate::backend::errors::Result<()> {
        let tx = self
            .tx
            .take()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        tx.commit().await?;
        Ok(())
    }

    async fn rollback(mut self: Box<Self>) -> crate::backend::errors::Result<()> {
        let tx = self
            .tx
            .take()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        tx.rollback().await?;
        Ok(())
    }

    fn dialect(&self) -> Dialect {
        self.dialect
    }
}