// ============================================================================
// FluxORM v3.0 - Production ORM Framework
// Pure implementation without examples or demonstrations
// ============================================================================

pub mod backend;
pub mod query;
pub mod core;
pub mod driver;

use crate::backend::common_models::Value;
use crate::backend::errors::*;
use crate::backend::migration::{Migration, MigrationRunner, MigrationStatus};
use crate::core::executor::{BackendExecutor, Executor};
use crate::core::model::Model;
use crate::core::transaction::{DatabaseTransaction, TransactionExecutor};
use crate::driver::dialect::Dialect;
use crate::driver::model::DatabaseBackend;
use crate::driver::postgres::PostgresBackend;
use crate::driver::sqlite::SqliteBackend;
use crate::query::builder::Query;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use crate::driver::mysql::MySqlBackend;

pub struct FluxConnection {
    backend: Arc<dyn DatabaseBackend>,
    tx: Option<Box<dyn DatabaseTransaction>>,
}

impl FluxConnection {
    fn new(backend: Arc<dyn DatabaseBackend>) -> Self {
        Self { backend, tx: None }
    }

    fn with_transaction(
        backend: Arc<dyn DatabaseBackend>,
        tx: Box<dyn DatabaseTransaction>,
    ) -> Self {
        Self {
            backend,
            tx: Some(tx),
        }
    }

    async fn get_executor(&mut self) -> Box<dyn Executor + '_> {
        if let Some(ref mut tx) = self.tx {
            Box::new(TransactionExecutor::new(tx))
        } else {
            Box::new(BackendExecutor::new(self.backend.as_ref()))
        }
    }

    fn dialect(&self) -> Dialect {
        if let Some(ref tx) = self.tx {
            tx.dialect()
        } else {
            self.backend.dialect()
        }
    }

    async fn commit(mut self) -> Result<()> {
        if let Some(tx) = self.tx.take() {
            tx.commit().await
        } else {
            Ok(())
        }
    }

    async fn rollback(mut self) -> Result<()> {
        if let Some(tx) = self.tx.take() {
            tx.rollback().await
        } else {
            Ok(())
        }
    }
}
// ============================================================================
// FLUX DATABASE
// ============================================================================

pub struct Flux {
    backend: Arc<dyn DatabaseBackend>,
    config: FluxConfig,
}

#[derive(Clone)]
pub struct FluxConfig {
    pub query_logging: bool,
    pub auto_timestamps: bool,
    pub strict_mode: bool,
}

impl Default for FluxConfig {
    fn default() -> Self {
        Self {
            query_logging: false,
            auto_timestamps: true,
            strict_mode: false,
        }
    }
}

impl Flux {
    /// Get current connection (with transaction if active)
    async fn connection(&self) -> Result<FluxConnection> {
        Ok(FluxConnection::new(self.backend.clone()))
    }

    pub async fn sqlite(database_url: &str) -> Result<Self> {
        let backend = SqliteBackend::new(database_url).await?;
        Ok(Self {
            backend: Arc::new(backend),
            config: FluxConfig::default(),
        })
    }

    pub async fn postgres(database_url: &str) -> Result<Self> {
        let backend = PostgresBackend::new(database_url).await?;
        Ok(Self {
            backend: Arc::new(backend),
            config: FluxConfig::default(),
        })
    }

    pub async fn mysql(database_url: &str) -> Result<Self> {
        let backend = MySqlBackend::new(database_url).await?;
        Ok(Self {
            backend: Arc::new(backend),
            config: FluxConfig::default(),
        })
    }

    pub fn with_config(mut self, config: FluxConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_logging(mut self, enabled: bool) -> Self {
        self.config.query_logging = enabled;
        self
    }

    pub fn with_strict_mode(mut self, enabled: bool) -> Self {
        self.config.strict_mode = enabled;
        self
    }

    pub async fn ping(&self) -> Result<()> {
        self.backend.ping().await
    }

    pub async fn begin(&self) -> Result<FluxTransaction> {
        let tx = self.backend.begin_transaction().await?;
        Ok(FluxTransaction {
            backend: self.backend.clone(),
            tx: Some(tx),
            config: self.config.clone(),
            committed: false,
            rolled_back: false,
        })
    }

    pub async fn query<T: Model>(&self, query: Query<T>) -> Result<Vec<T>> {
        let query = query.with_dialect(self.backend.dialect());
        let sql = query.to_sql();
        let params = query.params();

        if self.config.query_logging {
            eprintln!("[FLUX] SQL: {}", sql);
            eprintln!("[FLUX] Params: {:?}", params);
        }

        let rows = self.backend.fetch_all(&sql, params).await?;
        rows.into_iter()
            .map(|values| T::from_values(values))
            .collect()
    }

    pub async fn insert_with_new_tx<T: Model>(&self, model: T) -> Result<T> {
        let tx = self.backend.begin_transaction().await?;

        match self.insert(model).await {
            Ok(result) => {
                tx.commit().await?;
                Ok(result)
            }
            Err(e) => {
                let _ = tx.rollback().await;
                Err(e)
            }
        }
    }

    pub async fn insert<T: Model>(&self, mut model: T) -> Result<T> {
        if self.config.strict_mode {
            model.validate()?;
        }

        model.before_create(self).await?;

        let values = model.to_values();
        let model_id_is_none = model.id().is_none();
        let columns: Vec<String> = values.keys().cloned().collect();

        let values_list: Vec<Value> = columns
            .iter()
            .filter_map(|col| values.get(col).cloned())
            .collect();

        let dialect = self.backend.dialect();

        let placeholders: Vec<String> = (1..=values_list.len())
            .map(|i| dialect.placeholder(i))
            .collect();

        let mut sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            T::TABLE,
            columns.join(", "),
            placeholders.join(", ")
        );

        if self.config.query_logging {
            eprintln!("[FLUX] SQL: {}", sql);
        }

        // For PostgreSQL, use RETURNING clause to get the ID in one query
        if model_id_is_none && dialect == Dialect::PostgreSQL {
            sql.push_str(&format!(" RETURNING {}", T::PRIMARY_KEY));

            let id_row = self.backend.fetch_one(&sql, &values_list).await?;

            if let Some(id_value) = id_row.get(T::PRIMARY_KEY) {
                if let Ok(typed_id) = T::Id::try_from(id_value.clone()) {
                    model.set_id(typed_id);
                }
            }
        } else {
            // For SQLite and MySQL, use the old method
            self.backend.execute(&sql, &values_list).await?;

            if model_id_is_none {
                let id_sql = match dialect {
                    Dialect::SQLite => "SELECT last_insert_rowid() as id",
                    Dialect::PostgreSQL => {
                        // This shouldn't happen due to the check above, but just in case
                        &format!(
                            "SELECT currval(pg_get_serial_sequence('{}', '{}')) as id",
                            T::TABLE,
                            T::PRIMARY_KEY
                        )
                    }
                    Dialect::MySQL => "SELECT LAST_INSERT_ID() as id",
                    Dialect::MSSQL => unimplemented!()
                };

                let id_row = self.backend.fetch_one(id_sql, &[]).await?;

                if let Some(id_value) = id_row.get("id") {
                    if let Ok(typed_id) = T::Id::try_from(id_value.clone()) {
                        model.set_id(typed_id);
                    }
                }
            }
        }

        model.after_create(self).await?;
        Ok(model)
    }

    pub async fn update<T: Model>(&self, mut model: T) -> Result<()> {
        let id = model.id().ok_or(FluxError::NoId)?;

        if self.config.strict_mode {
            model.validate()?;
        }

        model.before_update(self).await?;

        let values = model.to_values();
        let mut assignments = Vec::new();
        let mut params = Vec::new();
        let dialect = self.backend.dialect();

        let mut param_idx = 1;
        for (col, val) in values {
            if col != T::PRIMARY_KEY {
                assignments.push(format!("{} = {}", col, dialect.placeholder(param_idx)));
                params.push(val);
                param_idx += 1;
            }
        }

        params.push(id.into());

        let sql = format!(
            "UPDATE {} SET {} WHERE {} = {}",
            T::TABLE,
            assignments.join(", "),
            T::PRIMARY_KEY,
            dialect.placeholder(param_idx)
        );

        if self.config.query_logging {
            eprintln!("[FLUX] SQL: {}", sql);
        }

        self.backend.execute(&sql, &params).await?;

        model.after_update(self).await?;
        Ok(())
    }

    pub async fn delete<T: Model>(&self, model: T) -> Result<()> {
        let id = model.id().ok_or(FluxError::NoId)?;

        model.before_delete(self).await?;

        let dialect = self.backend.dialect();
        let sql = format!(
            "DELETE FROM {} WHERE {} = {}",
            T::TABLE,
            T::PRIMARY_KEY,
            dialect.placeholder(1)
        );

        if self.config.query_logging {
            eprintln!("[FLUX] SQL: {}", sql);
        }

        self.backend.execute(&sql, &[id.into()]).await?;

        model.after_delete(self).await?;
        Ok(())
    }

    pub async fn find<T: Model>(&self, id: T::Id) -> Result<Option<T>> {
        let results = self
            .query(
                Query::<T>::new()
                    .with_dialect(self.backend.dialect())
                    .where_eq(T::PRIMARY_KEY, id)
                    .limit(1),
            )
            .await?;
        Ok(results.into_iter().next())
    }

    pub async fn find_or_fail<T: Model>(&self, id: T::Id) -> Result<T> {
        self.find(id).await?.ok_or(FluxError::NotFound)
    }

    pub async fn all<T: Model>(&self) -> Result<Vec<T>> {
        self.query(Query::<T>::new()).await
    }

    pub async fn count<T: Model>(&self, query: Query<T>) -> Result<i64> {
        let query = query.with_dialect(self.backend.dialect());

        // Build COUNT query manually to avoid string replacement issues
        let mut parts = vec![
            "SELECT COUNT(*) as count".to_string(),
            "FROM".to_string(),
            query.table().to_string(),
        ];

        if !query.where_conditions().is_empty() {
            parts.push("WHERE".to_string());
            parts.push(query.where_conditions().join(" AND "));
        }

        let sql = parts.join(" ");

        if self.config.query_logging {
            eprintln!("[FLUX] COUNT SQL: {}", sql);
            eprintln!("[FLUX] Params: {:?}", query.params());
        }

        let row = self.backend.fetch_one(&sql, query.params()).await?;

        if let Some(count_value) = row.get("count") {
            // Try different numeric types that databases might return
            if let Some(val) = count_value.as_i64() {
                return Ok(val);
            }
            if let Some(val) = count_value.as_i32() {
                return Ok(val as i64);
            }
            if let Some(val) = count_value.as_i16() {
                return Ok(val as i64);
            }

            Err(FluxError::Serialization(format!(
                "Cannot convert count value {:?} to i64",
                count_value
            )))
        } else {
            Ok(0)
        }
    }

    pub async fn exists<T: Model>(&self, query: Query<T>) -> Result<bool> {
        let count = self.count(query).await?;
        Ok(count > 0)
    }

    pub async fn first<T: Model>(&self, query: Query<T>) -> Result<Option<T>> {
        let results = self.query(query.limit(1)).await?;
        Ok(results.into_iter().next())
    }

    pub async fn paginate<T: Model>(
        &self,
        query: Query<T>,
        page: usize,
        per_page: usize,
    ) -> Result<Paginated<T>> {
        if page == 0 || per_page == 0 {
            return Err(FluxError::QueryBuild(
                "Page and per_page must be greater than 0".into(),
            ));
        }

        let total = self.count(query.clone()).await?;
        let items = self
            .query(query.limit(per_page).offset((page - 1) * per_page))
            .await?;

        Ok(Paginated {
            items,
            page,
            per_page,
            total,
            total_pages: ((total as usize + per_page - 1) / per_page).max(1),
        })
    }

    pub async fn transaction<F, R>(&self, f: F) -> Result<R>
    where
        F: for<'a> FnOnce(
                &'a mut Box<dyn DatabaseTransaction>,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<R>> + Send + 'a>,
            > + Send,
        R: Send,
    {
        let mut tx = self.backend.begin_transaction().await?;

        match f(&mut tx).await {
            Ok(result) => {
                tx.commit().await?;
                Ok(result)
            }
            Err(e) => {
                let _ = tx.rollback().await;
                Err(e)
            }
        }
    }

    pub async fn migrate(&self, migrations: &[Migration]) -> Result<()> {
        let runner = MigrationRunner::new(self);
        runner.run(migrations).await
    }

    pub async fn rollback_migrations(&self, migrations: &[Migration], steps: usize) -> Result<()> {
        let runner = MigrationRunner::new(self);
        runner.rollback(migrations, steps).await
    }

    pub async fn migration_status(&self) -> Result<Vec<MigrationStatus>> {
        let runner = MigrationRunner::new(self);
        runner.status().await
    }

    pub async fn raw_query<T: Model>(&self, sql: &str, params: &[Value]) -> Result<Vec<T>> {
        if self.config.query_logging {
            eprintln!("[FLUX] RAW SQL: {}", sql);
        }

        let rows = self.backend.fetch_all(sql, params).await?;
        rows.into_iter()
            .map(|values| T::from_values(values))
            .collect()
    }

    pub async fn raw_execute(&self, sql: &str, params: &[Value]) -> Result<u64> {
        if self.config.query_logging {
            eprintln!("[FLUX] RAW SQL: {}", sql);
        }

        self.backend.execute(sql, params).await
    }

    pub async fn batch_insert<T: Model>(&self, mut models: Vec<T>) -> Result<Vec<T>> {
        if models.is_empty() {
            return Ok(Vec::new());
        }

        // Проверка strict mode и хуков
        for model in &mut models {
            if self.config.strict_mode {
                model.validate()?;
            }
            model.before_create(self).await?;
        }

        let first_values = models[0].to_values();
        let mut columns: Vec<String> = first_values.keys().cloned().collect();
        columns.sort(); // гарантируем порядок

        let dialect = self.backend.dialect();
        let mut params: Vec<Value> = Vec::new();
        let mut value_groups: Vec<String> = Vec::new();
        let mut param_index = 0;

        for model in &models {
            let values = model.to_values();
            let mut placeholders = Vec::new();
            for col in &columns {
                let val = values.get(col).cloned().unwrap_or(Value::Null);
                param_index += 1;
                placeholders.push(dialect.placeholder(param_index));
                params.push(val);
            }
            value_groups.push(format!("({})", placeholders.join(", ")));
        }

        // Основной SQL-запрос
        let mut sql = format!(
            "INSERT INTO {} ({}) VALUES {}",
            T::TABLE,
            columns.join(", "),
            value_groups.join(", ")
        );

        let mut returning = false;

        if let Dialect::PostgreSQL = dialect {
            sql.push_str(" RETURNING *");
            returning = true;
        }

        if self.config.query_logging {
            eprintln!("[FLUX] SQL: {}", sql);
            eprintln!("[FLUX] Params: {:?}", params);
        }

        let results = if returning {
            // PostgreSQL — возвращаем все вставленные строки
            let rows = self.backend.fetch_all(&sql, &params).await?;
            let mut inserted = Vec::with_capacity(rows.len());
            for row in rows {
                let mut model = T::from_values(row)?;
                model.after_create(self).await?;
                inserted.push(model);
            }
            inserted
        } else {
            // SQLite/MySQL — просто выполняем и восстанавливаем ID по last_insert_id
            self.backend.execute(&sql, &params).await?;

            if models[0].id().is_none() {
                let id_sql = match dialect {
                    Dialect::SQLite => "SELECT last_insert_rowid() as id",
                    Dialect::PostgreSQL => "SELECT lastval() as id",
                    Dialect::MySQL => "SELECT LAST_INSERT_ID() as id",
                    Dialect::MSSQL => unimplemented!()
                };
                let id_row = self.backend.fetch_one(id_sql, &[]).await?;
                if let Some(id_val) = id_row.get("id") {
                    if let Ok(last_id) = T::Id::try_from(id_val.clone()) {
                        // Присваиваем ID по порядку (упрощённо)
                        let mut current_id = last_id.clone();
                        for model in &mut models {
                            model.set_id(current_id.clone());
                            model.after_create(self).await?;
                        }
                    }
                }
            } else {
                for model in &mut models {
                    model.after_create(self).await?;
                }
            }

            models
        };

        Ok(results)
    }

    pub async fn upsert<T: Model>(&self, model: T) -> Result<T> {
        if model.id().is_some() {
            self.update(model.clone()).await?;
            Ok(model)
        } else {
            self.insert(model).await
        }
    }
}

// ============================================================================
// FLUX TRANSACTION - Dedicated transaction instance
// ============================================================================

pub struct FluxTransaction {
    backend: Arc<dyn DatabaseBackend>,
    tx: Option<Box<dyn DatabaseTransaction>>,
    config: FluxConfig,
    committed: bool,
    rolled_back: bool,
}

impl FluxTransaction {
    fn dialect(&self) -> Dialect {
        self.tx
            .as_ref()
            .map(|t| t.dialect())
            .unwrap_or(self.backend.dialect())
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64> {
        if let Some(ref tx) = self.tx {
            // Use unsafe to get mutable reference - this is safe because
            // we're behind an async boundary and Rust's borrow checker
            // can't see across await points
            let tx_ptr =
                tx.as_ref() as *const dyn DatabaseTransaction as *mut dyn DatabaseTransaction;
            unsafe { (*tx_ptr).execute(sql, params).await }
        } else {
            Err(FluxError::Transaction(
                "Transaction already completed".into(),
            ))
        }
    }

    async fn fetch_one(&self, sql: &str, params: &[Value]) -> Result<HashMap<String, Value>> {
        if let Some(ref tx) = self.tx {
            let tx_ptr =
                tx.as_ref() as *const dyn DatabaseTransaction as *mut dyn DatabaseTransaction;
            unsafe { (*tx_ptr).fetch_one(sql, params).await }
        } else {
            Err(FluxError::Transaction(
                "Transaction already completed".into(),
            ))
        }
    }

    async fn fetch_all(&self, sql: &str, params: &[Value]) -> Result<Vec<HashMap<String, Value>>> {
        if let Some(ref tx) = self.tx {
            let tx_ptr =
                tx.as_ref() as *const dyn DatabaseTransaction as *mut dyn DatabaseTransaction;
            unsafe { (*tx_ptr).fetch_all(sql, params).await }
        } else {
            Err(FluxError::Transaction(
                "Transaction already completed".into(),
            ))
        }
    }

    pub async fn insert<T: Model>(&self, mut model: T) -> Result<T> {
        if self.config.strict_mode {
            model.validate()?;
        }

        // Create a temporary Flux instance for hooks
        let temp_flux = Flux {
            backend: self.backend.clone(),
            config: self.config.clone(),
        };

        model.before_create(&temp_flux).await?;

        let values = model.to_values();
        let model_id_is_none = model.id().is_none();
        let columns: Vec<String> = values.keys().cloned().collect();

        let values_list: Vec<Value> = columns
            .iter()
            .filter_map(|col| values.get(col).cloned())
            .collect();

        let dialect = self.dialect();
        let placeholders: Vec<String> = (1..=values_list.len())
            .map(|i| dialect.placeholder(i))
            .collect();

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            T::TABLE,
            columns.join(", "),
            placeholders.join(", ")
        );

        if self.config.query_logging {
            eprintln!("[FLUX TX] SQL: {}", sql);
        }

        self.execute(&sql, &values_list).await?;

        if model_id_is_none {
            let id_sql = match dialect {
                Dialect::SQLite => "SELECT last_insert_rowid() as id",
                Dialect::PostgreSQL => "SELECT lastval() as id",
                Dialect::MySQL => "SELECT LAST_INSERT_ID() as id",
                Dialect::MSSQL => unimplemented!()
            };

            let id_row = self.fetch_one(id_sql, &[]).await?;

            if let Some(id_value) = id_row.get("id") {
                if let Ok(typed_id) = T::Id::try_from(id_value.clone()) {
                    model.set_id(typed_id);
                }
            }
        }

        model.after_create(&temp_flux).await?;
        Ok(model)
    }

    pub async fn update<T: Model>(&self, mut model: T) -> Result<()> {
        let id = model.id().ok_or(FluxError::NoId)?;

        if self.config.strict_mode {
            model.validate()?;
        }

        let temp_flux = Flux {
            backend: self.backend.clone(),
            config: self.config.clone(),
        };

        model.before_update(&temp_flux).await?;

        let values = model.to_values();
        let mut assignments = Vec::new();
        let mut params = Vec::new();
        let dialect = self.dialect();

        let mut param_idx = 1;
        for (col, val) in values {
            if col != T::PRIMARY_KEY {
                assignments.push(format!("{} = {}", col, dialect.placeholder(param_idx)));
                params.push(val);
                param_idx += 1;
            }
        }

        params.push(id.into());

        let sql = format!(
            "UPDATE {} SET {} WHERE {} = {}",
            T::TABLE,
            assignments.join(", "),
            T::PRIMARY_KEY,
            dialect.placeholder(param_idx)
        );

        if self.config.query_logging {
            eprintln!("[FLUX TX] SQL: {}", sql);
        }

        self.execute(&sql, &params).await?;
        model.after_update(&temp_flux).await?;
        Ok(())
    }

    pub async fn delete<T: Model>(&self, model: T) -> Result<()> {
        let id = model.id().ok_or(FluxError::NoId)?;

        let temp_flux = Flux {
            backend: self.backend.clone(),
            config: self.config.clone(),
        };

        model.before_delete(&temp_flux).await?;

        let dialect = self.dialect();
        let sql = format!(
            "DELETE FROM {} WHERE {} = {}",
            T::TABLE,
            T::PRIMARY_KEY,
            dialect.placeholder(1)
        );

        if self.config.query_logging {
            eprintln!("[FLUX TX] SQL: {}", sql);
        }

        self.execute(&sql, &[id.into()]).await?;
        model.after_delete(&temp_flux).await?;
        Ok(())
    }

    pub async fn query<T: Model>(&self, query: Query<T>) -> Result<Vec<T>> {
        let query = query.with_dialect(self.dialect());
        let sql = query.to_sql();
        let params = query.params();

        if self.config.query_logging {
            eprintln!("[FLUX TX] SQL: {}", sql);
            eprintln!("[FLUX TX] Params: {:?}", params);
        }

        let rows = self.fetch_all(&sql, params).await?;
        rows.into_iter()
            .map(|values| T::from_values(values))
            .collect()
    }

    pub async fn commit(mut self) -> Result<()> {
        if self.committed || self.rolled_back {
            return Err(FluxError::Transaction(
                "Transaction already completed".into(),
            ));
        }

        let tx = self
            .tx
            .take()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;

        tx.commit().await?;
        self.committed = true;
        Ok(())
    }

    pub async fn rollback(mut self) -> Result<()> {
        if self.committed || self.rolled_back {
            return Err(FluxError::Transaction(
                "Transaction already completed".into(),
            ));
        }

        let tx = self
            .tx
            .take()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;

        tx.rollback().await?;
        self.rolled_back = true;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Paginated<T> {
    pub items: Vec<T>,
    pub page: usize,
    pub per_page: usize,
    pub total: i64,
    pub total_pages: usize,
}

impl<T> Paginated<T> {
    pub fn has_next(&self) -> bool {
        self.page < self.total_pages
    }

    pub fn has_prev(&self) -> bool {
        self.page > 1
    }

    pub fn next_page(&self) -> Option<usize> {
        if self.has_next() {
            Some(self.page + 1)
        } else {
            None
        }
    }

    pub fn prev_page(&self) -> Option<usize> {
        if self.has_prev() {
            Some(self.page - 1)
        } else {
            None
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
}