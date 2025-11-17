// ============================================================================
// FluxORM v3.0 - Production ORM Framework
// Pure implementation without examples or demonstrations
// ============================================================================

pub mod backend;

use crate::backend::bind_param::{bind_params_mysql, bind_params_pg, bind_params_sqlite};
use crate::backend::common_models::Value;
use crate::backend::errors::*;
use crate::backend::row_mapper::{row_to_map_mysql, row_to_map_pg, row_to_map_sqlite};
// ИСПОЛЬЗУЕТСЯ ДЛЯ РЕАЛИЗАЦИИ TIMESTAMPS
use crate::backend::where_models::Operator;
use crate::backend::where_models::Operator::{
    Between, Equals, GreaterThan, GreaterThanOrEquals, In, IsNotNull, IsNull, LessThan,
    LessThanOrEquals, Like, NotEquals, NotIn,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{
    Transaction as SqlxTransaction,
    mysql::{MySqlPool, MySqlPoolOptions},
    postgres::{PgPool, PgPoolOptions},
    sqlite::{SqlitePool, SqlitePoolOptions},
};
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::task_local;

// Thread-local transaction context
task_local! {
    static TRANSACTION_CONTEXT: RefCell<Option<TransactionContext>>;
}

struct TransactionContext {
    tx: Box<dyn DatabaseTransaction>,
}

// ============================================================================
// VALUE SYSTEM
// ============================================================================

// ============================================================================
// DIALECT
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    SQLite,
    PostgreSQL,
    MySQL,
}

impl Dialect {
    pub fn get_placeholder(&self) -> &'static str {
        match self {
            Dialect::PostgreSQL => "$",
            Dialect::MySQL | Dialect::SQLite => "?",
        }
    }
    pub fn placeholder(&self, index: usize) -> String {
        match self {
            Dialect::PostgreSQL => format!("${}", index),
            Dialect::MySQL | Dialect::SQLite => "?".to_string(),
        }
    }

    pub fn quote_identifier(&self, ident: &str) -> String {
        match self {
            Dialect::PostgreSQL => format!("\"{}\"", ident.replace('\"', "\"\"")),
            Dialect::MySQL => format!("`{}`", ident.replace('`', "``")),
            Dialect::SQLite => format!("\"{}\"", ident.replace('\"', "\"\"")),
        }
    }

    pub fn returning_clause(&self) -> &str {
        match self {
            Dialect::PostgreSQL => " RETURNING *",
            Dialect::MySQL | Dialect::SQLite => "",
        }
    }

    pub fn supports_returning(&self) -> bool {
        matches!(self, Dialect::PostgreSQL)
    }

    pub fn limit_clause(&self, limit: usize, offset: Option<usize>) -> String {
        match offset {
            Some(off) => format!("LIMIT {} OFFSET {}", limit, off),
            None => format!("LIMIT {}", limit),
        }
    }
}

#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64>;
    async fn fetch_one(&mut self, sql: &str, params: &[Value]) -> Result<HashMap<String, Value>>;
    async fn fetch_all(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>>;
    async fn fetch_optional(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<Option<HashMap<String, Value>>>;
    fn dialect(&self) -> Dialect;
}

struct TransactionExecutor<'a> {
    tx: &'a mut Box<dyn DatabaseTransaction>,
}

#[async_trait]
impl<'a> Executor for TransactionExecutor<'a> {
    async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64> {
        self.tx.execute(sql, params).await
    }

    async fn fetch_one(&mut self, sql: &str, params: &[Value]) -> Result<HashMap<String, Value>> {
        self.tx.fetch_one(sql, params).await
    }

    async fn fetch_all(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>> {
        self.tx.fetch_all(sql, params).await
    }

    async fn fetch_optional(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<Option<HashMap<String, Value>>> {
        self.tx.fetch_optional(sql, params).await
    }

    fn dialect(&self) -> Dialect {
        self.tx.dialect()
    }
}

// Wrapper for DatabaseBackend to implement Executor
struct BackendExecutor<'a> {
    backend: &'a dyn DatabaseBackend,
}

#[async_trait]
impl<'a> Executor for BackendExecutor<'a> {
    async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64> {
        self.backend.execute(sql, params).await
    }

    async fn fetch_one(&mut self, sql: &str, params: &[Value]) -> Result<HashMap<String, Value>> {
        self.backend.fetch_one(sql, params).await
    }

    async fn fetch_all(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>> {
        self.backend.fetch_all(sql, params).await
    }

    async fn fetch_optional(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<Option<HashMap<String, Value>>> {
        self.backend.fetch_optional(sql, params).await
    }

    fn dialect(&self) -> Dialect {
        self.backend.dialect()
    }
}

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
            Box::new(TransactionExecutor { tx })
        } else {
            Box::new(BackendExecutor {
                backend: self.backend.as_ref(),
            })
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
// DATABASE BACKEND TRAIT
// ============================================================================

#[async_trait]
pub trait DatabaseBackend: Send + Sync {
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64>;
    async fn fetch_one(&self, sql: &str, params: &[Value]) -> Result<HashMap<String, Value>>;
    async fn fetch_all(&self, sql: &str, params: &[Value]) -> Result<Vec<HashMap<String, Value>>>;
    async fn fetch_optional(
        &self,
        sql: &str,
        params: &[Value],
    ) -> Result<Option<HashMap<String, Value>>>;
    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>>;
    fn dialect(&self) -> Dialect;
    async fn ping(&self) -> Result<()>;
}

#[async_trait]
pub trait DatabaseTransaction: Send + Sync {
    async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64>;
    async fn fetch_one(&mut self, sql: &str, params: &[Value]) -> Result<HashMap<String, Value>>;
    async fn fetch_all(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>>;
    async fn fetch_optional(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<Option<HashMap<String, Value>>>;
    async fn commit(self: Box<Self>) -> Result<()>;
    async fn rollback(self: Box<Self>) -> Result<()>;
    fn dialect(&self) -> Dialect;
}

// ============================================================================
// SQLITE BACKEND
// ============================================================================

pub struct SqliteBackend {
    pool: SqlitePool,
}

impl SqliteBackend {
    pub async fn new(database_url: &str) -> Result<Self> {
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
    ) -> Result<Self> {
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
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let result = query.execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    async fn fetch_one(&self, sql: &str, params: &[Value]) -> Result<HashMap<String, Value>> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let row = query.fetch_one(&self.pool).await?;
        row_to_map_sqlite(&row)
    }

    async fn fetch_all(&self, sql: &str, params: &[Value]) -> Result<Vec<HashMap<String, Value>>> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let rows = query.fetch_all(&self.pool).await?;
        rows.iter().map(row_to_map_sqlite).collect()
    }

    async fn fetch_optional(
        &self,
        sql: &str,
        params: &[Value],
    ) -> Result<Option<HashMap<String, Value>>> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let row = query.fetch_optional(&self.pool).await?;
        row.map(|r| row_to_map_sqlite(&r)).transpose()
    }

    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>> {
        let tx = self.pool.begin().await?;
        Ok(Box::new(SqliteTransactionWrapper {
            tx: Some(tx),
            dialect: Dialect::SQLite,
        }))
    }

    fn dialect(&self) -> Dialect {
        Dialect::SQLite
    }

    async fn ping(&self) -> Result<()> {
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
    async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = SqliteBackend::bind_params(query, params);
        let result = query.execute(&mut **tx).await?;
        Ok(result.rows_affected())
    }

    async fn fetch_one(&mut self, sql: &str, params: &[Value]) -> Result<HashMap<String, Value>> {
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
    ) -> Result<Vec<HashMap<String, Value>>> {
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
    ) -> Result<Option<HashMap<String, Value>>> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = SqliteBackend::bind_params(query, params);
        let row = query.fetch_optional(&mut **tx).await?;
        row.map(|r| row_to_map_sqlite(&r)).transpose()
    }

    async fn commit(mut self: Box<Self>) -> Result<()> {
        let tx = self
            .tx
            .take()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        tx.commit().await?;
        Ok(())
    }

    async fn rollback(mut self: Box<Self>) -> Result<()> {
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

// ============================================================================
// MODEL TRAIT
// ============================================================================

#[async_trait]
pub trait Model: Sized + Send + Sync + Clone + Any + 'static {
    const TABLE: &'static str;
    const PRIMARY_KEY: &'static str = "id";

    type Id: Clone + Send + Sync + Into<Value> + TryFrom<Value, Error = FluxError>;

    fn id(&self) -> Option<Self::Id>;
    fn set_id(&mut self, id: Self::Id);
    fn to_values(&self) -> HashMap<String, Value>;
    fn from_values(values: HashMap<String, Value>) -> Result<Self>;

    fn validate(&self) -> Result<()> {
        Ok(())
    }

    async fn before_create(&mut self, _db: &Flux) -> Result<()> {
        Ok(())
    }
    async fn after_create(&self, _db: &Flux) -> Result<()> {
        Ok(())
    }
    async fn before_update(&mut self, _db: &Flux) -> Result<()> {
        Ok(())
    }
    async fn after_update(&self, _db: &Flux) -> Result<()> {
        Ok(())
    }
    async fn before_delete(&self, _db: &Flux) -> Result<()> {
        Ok(())
    }
    async fn after_delete(&self, _db: &Flux) -> Result<()> {
        Ok(())
    }
}

// ============================================================================
// QUERY BUILDER
// ============================================================================

#[derive(Clone)]
pub struct WhereClauseMetadata {
    field: String,
    operation: Operator,
    indexes: Vec<usize>,
}

const PLACEHOLDER_PATTERN: &'static str = "_<_@_#_PLC_HOLDER_#_@_>_";

impl WhereClauseMetadata {
    fn new(field: &str, operation: Operator, indexes: Vec<usize>) -> Self {
        Self {
            field: field.to_string(),
            operation,
            indexes,
        }
    }

    fn new_with_default(field: &str, operation: Operator) -> Self {
        Self::new(field, operation, vec![])
    }

    pub fn build_condition(&self, dialect: Dialect) -> String {
        let quoted_field = dialect.quote_identifier(&self.field);
        let template = self.operation.template();

        match self.operation {
            Operator::IsNull | Operator::IsNotNull => {
                // Эти операторы не требуют placeholder'ов
                template.replacen("{}", &quoted_field, 1)
            }
            Operator::Between => {
                // BETWEEN требует два placeholder'а
                if self.indexes.len() < 2 {
                    panic!("Between operation requires 2 indexes");
                }
                let ph1 = dialect.placeholder(self.indexes[0]);
                let ph2 = dialect.placeholder(self.indexes[1]);

                template
                    .replacen("{}", &quoted_field, 1)
                    .replacen("{}", &ph1, 1)
                    .replacen("{}", &ph2, 1)
            }
            Operator::In | Operator::NotIn => {
                // IN/NOT IN может требовать несколько placeholder'ов
                if self.indexes.is_empty() {
                    panic!("IN/NOT IN operation requires at least one index");
                }

                let placeholders: Vec<String> = self
                    .indexes
                    .iter()
                    .map(|&idx| dialect.placeholder(idx))
                    .collect();
                let placeholders_str = placeholders.join(", ");

                template
                    .replacen("{}", &quoted_field, 1)
                    .replacen("{}", &placeholders_str, 1)
            }
            _ => {
                // Все остальные операторы требуют один placeholder
                if self.indexes.is_empty() {
                    panic!("Operation requires at least one index");
                }
                let placeholder = dialect.placeholder(self.indexes[0]);

                template
                    .replacen("{}", &quoted_field, 1)
                    .replacen("{}", &placeholder, 1)
            }
        }
    }
}

#[derive(Clone)]
pub struct Query<T: Model> {
    table: String,
    select_cols: Vec<String>,
    where_conditions: Vec<String>,
    where_metadata: Vec<WhereClauseMetadata>,
    where_params: Vec<Value>,
    order_by: Vec<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    dialect: Option<Dialect>,
    _marker: PhantomData<T>,
}

macro_rules! impl_simple_where_methods {
    ($($name:ident => $op:expr),* $(,)?) => {
        $(
            pub fn $name<V: Into<Value>>(mut self, column: &str, value: V) -> Self {
                let idx = self.where_params.len() + 1;
                let placeholder = self.extract_placeholder(idx);

                let op = $op;
                let condition = op.template()
                    .replacen("{}", column, 1)
                    .replacen("{}", &placeholder, 1);

                self.where_conditions.push(condition);
                self.where_metadata.push(WhereClauseMetadata::new(column, op, vec![idx]));
                self.where_params.push(value.into());
                self
            }
        )*
    };
}

impl<T: Model> Query<T> {
    pub fn new() -> Self {
        Self {
            table: T::TABLE.to_string(),
            select_cols: vec!["*".to_string()],
            where_conditions: vec![],
            where_metadata: vec![],
            where_params: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
            dialect: None,
            _marker: PhantomData,
        }
    }

    pub fn with_dialect(mut self, dialect: Dialect) -> Self {
        let old_dialect = self.dialect;
        self.dialect = Some(dialect);

        // Пересобираем только если диалект изменился
        if old_dialect != self.dialect && !self.where_conditions.is_empty() {
            self.rebuild_where_conditions();
        }

        self
    }

    fn rebuild_where_conditions(&mut self) {
        self.where_conditions.clear();

        for meta in &self.where_metadata {
            self.where_conditions
                .push(meta.build_condition(self.dialect.unwrap()))
        }
    }

    pub fn select(mut self, cols: &[&str]) -> Self {
        self.select_cols = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    impl_simple_where_methods! {
        where_eq => Operator::Equals,
        where_ne => Operator::NotEquals,
        where_gt => Operator::GreaterThan,
        where_gte => Operator::GreaterThanOrEquals,
        where_lt => Operator::LessThan,
        where_lte => Operator::LessThanOrEquals,
        where_like => Operator::Like,
    }

    pub fn where_in<V: Into<Value>>(mut self, column: &str, values: Vec<V>) -> Self {
        if values.is_empty() {
            return self;
        }

        let start_idx = self.where_params.len() + 1;
        let placeholders: Vec<String> = (0..values.len())
            .map(|i| self.extract_placeholder(start_idx + i))
            .collect();

        self.where_metadata.push(WhereClauseMetadata::new(
            column,
            In,
            (start_idx..values.len() + 1).collect(),
        ));
        self.where_conditions
            .push(format!("{} IN ({})", column, placeholders.join(", ")));

        for val in values {
            self.where_params.push(val.into());
        }
        self
    }

    pub fn where_not_in<V: Into<Value>>(mut self, column: &str, values: Vec<V>) -> Self {
        if values.is_empty() {
            return self;
        }

        let start_idx = self.where_params.len() + 1;
        let placeholders: Vec<String> = (0..values.len())
            .map(|i| self.extract_placeholder(start_idx + i))
            .collect();

        self.where_conditions
            .push(format!("{} NOT IN ({})", column, placeholders.join(", ")));
        self.where_metadata.push(WhereClauseMetadata::new(
            column,
            NotIn,
            (start_idx..values.len() + 1).collect(),
        ));
        for val in values {
            self.where_params.push(val.into());
        }
        self
    }

    pub fn where_null(mut self, column: &str) -> Self {
        self.where_conditions.push(format!("{} IS NULL", column));
        self.where_metadata
            .push(WhereClauseMetadata::new_with_default(column, IsNull));
        self
    }

    pub fn where_not_null(mut self, column: &str) -> Self {
        self.where_conditions
            .push(format!("{} IS NOT NULL", column));
        self.where_metadata
            .push(WhereClauseMetadata::new_with_default(column, IsNotNull));
        self
    }

    pub fn where_between<V: Into<Value>>(mut self, column: &str, start: V, end: V) -> Self {
        let idx1 = self.where_params.len() + 1;
        let idx2 = idx1 + 1;
        let ph1 = self.extract_placeholder(idx1);
        let ph2 = self.extract_placeholder(idx2);

        self.where_conditions
            .push(format!("{} BETWEEN {} AND {}", column, ph1, ph2));
        self.where_metadata
            .push(WhereClauseMetadata::new(column, Between, vec![idx1, idx2]));
        self.where_params.push(start.into());
        self.where_params.push(end.into());
        self
    }

    fn extract_placeholder(&self, idx: usize) -> String {
        self.dialect
            .map(|d| d.placeholder(idx))
            .unwrap_or_else(|| "?".to_string())
    }

    pub fn order_by(mut self, column: &str) -> Self {
        self.order_by.push(format!("{} ASC", column));
        self
    }

    pub fn order_by_desc(mut self, column: &str) -> Self {
        self.order_by.push(format!("{} DESC", column));
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn offset(mut self, n: usize) -> Self {
        self.offset = Some(n);
        self
    }

    pub fn to_sql(&self) -> String {
        let mut parts = vec![
            "SELECT".to_string(),
            self.select_cols.join(", "),
            "FROM".to_string(),
            self.table.clone(),
        ];

        if !self.where_conditions.is_empty() {
            parts.push("WHERE".to_string());
            parts.push(self.where_conditions.join(" AND "));
        }

        if !self.order_by.is_empty() {
            parts.push("ORDER BY".to_string());
            parts.push(self.order_by.join(", "));
        }

        if let Some(limit) = self.limit {
            parts.push(format!("LIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            parts.push(format!("OFFSET {}", offset));
        }

        parts.join(" ")
    }

    pub fn params(&self) -> &[Value] {
        &self.where_params
    }
}

impl<T: Model> Default for Query<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// RELATION TRAITS
// ============================================================================

#[async_trait]
pub trait HasMany<T: Model>: Model {
    fn foreign_key() -> &'static str;

    async fn load_many(&self, db: &Flux) -> Result<Vec<T>> {
        let id = self.id().ok_or(FluxError::NoId)?;
        db.query(Query::<T>::new().where_eq(Self::foreign_key(), id))
            .await
    }
}

#[async_trait]
pub trait HasOne<T: Model>: Model {
    fn foreign_key() -> &'static str;

    async fn load_one(&self, db: &Flux) -> Result<Option<T>> {
        let id = self.id().ok_or(FluxError::NoId)?;
        let results = db
            .query(Query::<T>::new().where_eq(Self::foreign_key(), id).limit(1))
            .await?;
        Ok(results.into_iter().next())
    }
}

#[async_trait]
pub trait BelongsTo<T: Model>: Model {
    fn foreign_key_value(&self) -> Option<T::Id>;

    async fn load_parent(&self, db: &Flux) -> Result<Option<T>> {
        if let Some(parent_id) = self.foreign_key_value() {
            db.find(parent_id).await
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
pub trait BelongsToMany<T: Model>: Model {
    fn pivot_table() -> &'static str;
    fn foreign_key() -> &'static str;
    fn related_key() -> &'static str;

    async fn load_many(&self, db: &Flux) -> Result<Vec<T>> {
        let id = self.id().ok_or(FluxError::NoId)?;
        let dialect = db.backend.dialect();

        let related_table = dialect.quote_identifier(T::TABLE);
        let related_pk = dialect.quote_identifier(T::PRIMARY_KEY);
        let pivot_table = dialect.quote_identifier(Self::pivot_table());
        let related_key = dialect.quote_identifier(Self::related_key());
        let foreign_key = dialect.quote_identifier(Self::foreign_key());

        let sql = format!(
            "SELECT {related_table}.*
         FROM {related_table}
         INNER JOIN {pivot_table} ON {related_table}.{related_pk} = {pivot_table}.{related_key}
         WHERE {pivot_table}.{foreign_key} = {placeholder}",
            related_table = related_table,
            pivot_table = pivot_table,
            related_pk = related_pk,
            related_key = related_key,
            foreign_key = foreign_key,
            placeholder = dialect.placeholder(1)
        );

        let rows = db.backend.fetch_all(&sql, &[id.into()]).await?;
        rows.into_iter()
            .map(|values| T::from_values(values))
            .collect()
    }

    async fn attach(&self, db: &Flux, related_id: T::Id) -> Result<()> {
        let id = self.id().ok_or(FluxError::NoId)?;
        let dialect = db.backend.dialect();

        let pivot_table = dialect.quote_identifier(Self::pivot_table());
        let foreign_key = dialect.quote_identifier(Self::foreign_key());
        let related_key = dialect.quote_identifier(Self::related_key());

        let sql = format!(
            "INSERT INTO {pivot_table} ({foreign_key}, {related_key}) VALUES ({placeholder1}, {placeholder2})",
            pivot_table = pivot_table,
            foreign_key = foreign_key,
            related_key = related_key,
            placeholder1 = dialect.placeholder(1),
            placeholder2 = dialect.placeholder(2)
        );

        db.backend
            .execute(&sql, &[id.into(), related_id.into()])
            .await?;

        Ok(())
    }

    async fn detach(&self, db: &Flux, related_id: T::Id) -> Result<()> {
        let id = self.id().ok_or(FluxError::NoId)?;
        let dialect = db.backend.dialect();

        let pivot_table = dialect.quote_identifier(Self::pivot_table());
        let foreign_key = dialect.quote_identifier(Self::foreign_key());
        let related_key = dialect.quote_identifier(Self::related_key());

        let sql = format!(
            "DELETE FROM {pivot_table} WHERE {foreign_key} = {placeholder1} AND {related_key} = {placeholder2}",
            pivot_table = pivot_table,
            foreign_key = foreign_key,
            related_key = related_key,
            placeholder1 = dialect.placeholder(1),
            placeholder2 = dialect.placeholder(2)
        );

        db.backend
            .execute(&sql, &[id.into(), related_id.into()])
            .await?;

        Ok(())
    }
}
// ============================================================================
// SOFT DELETE TRAIT
// ============================================================================

#[async_trait]
pub trait SoftDelete: Model {
    fn deleted_at(&self) -> Option<DateTime<Utc>>;
    fn set_deleted_at(&mut self, time: Option<DateTime<Utc>>);

    async fn soft_delete(&mut self, db: &Flux) -> Result<()> {
        self.set_deleted_at(Some(Utc::now()));
        self.before_delete(db).await?;
        db.update(self.clone()).await?;
        self.after_delete(db).await
    }

    async fn restore(&mut self, db: &Flux) -> Result<()> {
        self.set_deleted_at(None);
        db.update(self.clone()).await
    }

    async fn force_delete(self, db: &Flux) -> Result<()> {
        db.delete(self).await
    }

    fn with_trashed() -> Query<Self> {
        Query::new()
    }

    fn only_trashed() -> Query<Self> {
        Query::new().where_not_null("deleted_at")
    }
}

// ============================================================================
// TIMESTAMPS TRAIT
// ============================================================================

pub trait Timestamps: Model {
    fn created_at(&self) -> DateTime<Utc>;
    fn updated_at(&self) -> DateTime<Utc>;
    fn set_created_at(&mut self, time: DateTime<Utc>);
    fn set_updated_at(&mut self, time: DateTime<Utc>);

    fn touch(&mut self) {
        self.set_updated_at(Utc::now());
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
            query.table.clone(),
        ];

        if !query.where_conditions.is_empty() {
            parts.push("WHERE".to_string());
            parts.push(query.where_conditions.join(" AND "));
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
// ============================================================================
// POSTGRESQL BACKEND - Complete Implementation
// ============================================================================

pub struct PostgresBackend {
    pool: PgPool,
}

impl PostgresBackend {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .map_err(|e| FluxError::Pool(format!("PostgreSQL connection failed: {}", e)))?;
        Ok(Self { pool })
    }

    pub async fn with_pool_config(
        database_url: &str,
        max_connections: u32,
        min_connections: u32,
    ) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .connect(database_url)
            .await
            .map_err(|e| FluxError::Pool(format!("PostgreSQL connection failed: {}", e)))?;
        Ok(Self { pool })
    }

    fn bind_params<'q>(
        query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
        params: &'q [Value],
    ) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
        bind_params_pg(query, params)
    }
}

#[async_trait]
impl DatabaseBackend for PostgresBackend {
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let result = query.execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    async fn fetch_one(&self, sql: &str, params: &[Value]) -> Result<HashMap<String, Value>> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let row = query.fetch_one(&self.pool).await?;
        row_to_map_pg(&row)
    }

    async fn fetch_all(&self, sql: &str, params: &[Value]) -> Result<Vec<HashMap<String, Value>>> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let rows = query.fetch_all(&self.pool).await?;
        rows.iter().map(row_to_map_pg).collect()
    }

    async fn fetch_optional(
        &self,
        sql: &str,
        params: &[Value],
    ) -> Result<Option<HashMap<String, Value>>> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let row = query.fetch_optional(&self.pool).await?;
        row.map(|r| row_to_map_pg(&r)).transpose()
    }

    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>> {
        let tx = self.pool.begin().await?;
        Ok(Box::new(PostgresTransactionWrapper {
            tx: Some(tx),
            dialect: Dialect::PostgreSQL,
        }))
    }

    fn dialect(&self) -> Dialect {
        Dialect::PostgreSQL
    }

    async fn ping(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

struct PostgresTransactionWrapper {
    tx: Option<SqlxTransaction<'static, sqlx::Postgres>>,
    dialect: Dialect,
}

#[async_trait]
impl DatabaseTransaction for PostgresTransactionWrapper {
    async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = PostgresBackend::bind_params(query, params);
        let result = query.execute(&mut **tx).await?;
        Ok(result.rows_affected())
    }

    async fn fetch_one(&mut self, sql: &str, params: &[Value]) -> Result<HashMap<String, Value>> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = PostgresBackend::bind_params(query, params);
        let row = query.fetch_one(&mut **tx).await?;
        row_to_map_pg(&row)
    }

    async fn fetch_all(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = PostgresBackend::bind_params(query, params);
        let rows = query.fetch_all(&mut **tx).await?;
        rows.iter().map(row_to_map_pg).collect()
    }

    async fn fetch_optional(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<Option<HashMap<String, Value>>> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = PostgresBackend::bind_params(query, params);
        let row = query.fetch_optional(&mut **tx).await?;
        row.map(|r| row_to_map_pg(&r)).transpose()
    }

    async fn commit(mut self: Box<Self>) -> Result<()> {
        let tx = self
            .tx
            .take()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        tx.commit().await?;
        Ok(())
    }

    async fn rollback(mut self: Box<Self>) -> Result<()> {
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

// ============================================================================
// MYSQL BACKEND - Complete Implementation
// ============================================================================

pub struct MySqlBackend {
    pool: MySqlPool,
}

impl MySqlBackend {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .map_err(|e| FluxError::Pool(format!("MySQL connection failed: {}", e)))?;
        Ok(Self { pool })
    }

    pub async fn with_pool_config(
        database_url: &str,
        max_connections: u32,
        min_connections: u32,
    ) -> Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .connect(database_url)
            .await
            .map_err(|e| FluxError::Pool(format!("MySQL connection failed: {}", e)))?;
        Ok(Self { pool })
    }

    fn bind_params<'q>(
        query: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
        params: &'q [Value],
    ) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
        bind_params_mysql(query, params)
    }
}

#[async_trait]
impl DatabaseBackend for MySqlBackend {
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<u64> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let result = query.execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    async fn fetch_one(&self, sql: &str, params: &[Value]) -> Result<HashMap<String, Value>> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let row = query.fetch_one(&self.pool).await?;
        row_to_map_mysql(&row)
    }

    async fn fetch_all(&self, sql: &str, params: &[Value]) -> Result<Vec<HashMap<String, Value>>> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let rows = query.fetch_all(&self.pool).await?;
        rows.iter().map(row_to_map_mysql).collect()
    }

    async fn fetch_optional(
        &self,
        sql: &str,
        params: &[Value],
    ) -> Result<Option<HashMap<String, Value>>> {
        let query = sqlx::query(sql);
        let query = Self::bind_params(query, params);
        let row = query.fetch_optional(&self.pool).await?;
        row.map(|r| row_to_map_mysql(&r)).transpose()
    }

    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>> {
        let tx = self.pool.begin().await?;
        Ok(Box::new(MySqlTransactionWrapper {
            tx: Some(tx),
            dialect: Dialect::MySQL,
        }))
    }

    fn dialect(&self) -> Dialect {
        Dialect::MySQL
    }

    async fn ping(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

struct MySqlTransactionWrapper {
    tx: Option<SqlxTransaction<'static, sqlx::MySql>>,
    dialect: Dialect,
}

#[async_trait]
impl DatabaseTransaction for MySqlTransactionWrapper {
    async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = MySqlBackend::bind_params(query, params);
        let result = query.execute(&mut **tx).await?;
        Ok(result.rows_affected())
    }

    async fn fetch_one(&mut self, sql: &str, params: &[Value]) -> Result<HashMap<String, Value>> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = MySqlBackend::bind_params(query, params);
        let row = query.fetch_one(&mut **tx).await?;
        row_to_map_mysql(&row)
    }

    async fn fetch_all(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = MySqlBackend::bind_params(query, params);
        let rows = query.fetch_all(&mut **tx).await?;
        rows.iter().map(row_to_map_mysql).collect()
    }

    async fn fetch_optional(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<Option<HashMap<String, Value>>> {
        let tx = self
            .tx
            .as_mut()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        let query = sqlx::query(sql);
        let query = MySqlBackend::bind_params(query, params);
        let row = query.fetch_optional(&mut **tx).await?;
        row.map(|r| row_to_map_mysql(&r)).transpose()
    }

    async fn commit(mut self: Box<Self>) -> Result<()> {
        let tx = self
            .tx
            .take()
            .ok_or_else(|| FluxError::Transaction("Transaction already completed".into()))?;
        tx.commit().await?;
        Ok(())
    }

    async fn rollback(mut self: Box<Self>) -> Result<()> {
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

// ============================================================================
// TYPE-SAFE FIELD SYSTEM
// ============================================================================

pub trait Field<T: Model>: Send + Sync {
    fn name(&self) -> &'static str;
    type Type: Into<Value> + TryFrom<Value, Error = FluxError>;
}

/// Marker trait for fields that can be used in WHERE clauses
pub trait Comparable<T: Model>: Field<T> {}

/// Marker trait for fields that can be ordered
pub trait Orderable<T: Model>: Field<T> {}
/// Построитель условий полей для типобезопасных запросов
#[derive(Clone, Debug)]
pub struct FieldCondition<T: Model> {
    pub field_name: &'static str,
    pub operator: ConditionOperator,
    pub value: Value,
    pub _marker: std::marker::PhantomData<T>,
}

#[derive(Clone, Debug)]
pub enum ConditionOperator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    In(Vec<Value>),
    IsNull,
    IsNotNull,
}

impl<T: Model> FieldCondition<T> {
    /// Применить это условие к запросу
    pub fn apply_to(self, query: Query<T>) -> Query<T> {
        match self.operator {
            ConditionOperator::Eq => query.where_eq(self.field_name, self.value),
            ConditionOperator::Ne => query.where_ne(self.field_name, self.value),
            ConditionOperator::Gt => query.where_gt(self.field_name, self.value),
            ConditionOperator::Gte => query.where_gte(self.field_name, self.value),
            ConditionOperator::Lt => query.where_lt(self.field_name, self.value),
            ConditionOperator::Lte => query.where_lte(self.field_name, self.value),
            ConditionOperator::Like => query.where_like(self.field_name, self.value),
            ConditionOperator::In(values) => query.where_in(self.field_name, values),
            ConditionOperator::IsNull => query.where_null(self.field_name),
            ConditionOperator::IsNotNull => query.where_not_null(self.field_name),
        }
    }
}

/// Сортировка по полю для типобезопасных запросов
#[derive(Clone, Debug)]
pub struct FieldOrder<T: Model> {
    pub field_name: &'static str,
    pub descending: bool,
    pub _marker: std::marker::PhantomData<T>,
}

impl<T: Model> FieldOrder<T> {
    /// Применить эту сортировку к запросу
    pub fn apply_to(self, query: Query<T>) -> Query<T> {
        if self.descending {
            query.order_by_desc(self.field_name)
        } else {
            query.order_by(self.field_name)
        }
    }
}

/// Трейт расширения для использования условий полей с запросами
pub trait QueryFieldExt<T: Model>: Sized {
    /// Применить условие поля к этому запросу
    fn with_condition(self, condition: FieldCondition<T>) -> Self;

    /// Применить сортировку по полю к этому запросу
    fn with_order(self, order: FieldOrder<T>) -> Self;
}

impl<T: Model> QueryFieldExt<T> for Query<T> {
    fn with_condition(self, condition: FieldCondition<T>) -> Self {
        condition.apply_to(self)
    }

    fn with_order(self, order: FieldOrder<T>) -> Self {
        order.apply_to(self)
    }
}

macro_rules! impl_where_methods {
    ($($name:ident => $op:expr),* $(,)?) => {
        $(
            pub fn $name<F, V>(mut self, field: F, value: V) -> Self
            where
                F: Field<T>,
                V: Into<F::Type>,
            {
                let idx = self.where_params.len() + 1;
                let placeholder = self.extract_placeholder(idx);

                let op = $op;
                let condition = op.template()
                    .replacen("{}", field.name(), 1)
                    .replacen("{}", &placeholder, 1);

                self.where_conditions.push(condition);
                self.where_metadata.push(WhereClauseMetadata::new(field.name(), op, vec![idx]));
                let field_value: F::Type = value.into();
                self.where_params.push(field_value.into());
                self
            }
        )*
    };
}

impl<T: Model> Query<T> {
    /// Type-safe where clause using field references
    impl_where_methods! {
        where_field_eq => Equals,
        where_field_ne => NotEquals,
        where_field_gt => GreaterThan,
        where_field_gte => GreaterThanOrEquals,
        where_field_lt => LessThan,
        where_field_lte => LessThanOrEquals,
        where_field_like => Like
    }

    // остальные особые методы:
    pub fn where_field_in<F, V>(mut self, field: F, values: Vec<V>) -> Self
    where
        F: Field<T>,
        V: Into<F::Type>,
    {
        let start_idx = self.where_params.len() + 1;
        let placeholders: Vec<String> = (0..values.len())
            .map(|i| self.extract_placeholder(start_idx + i))
            .collect();
        self.where_conditions
            .push(format!("{} IN ({})", field.name(), placeholders.join(", ")));

        self.where_metadata.push(WhereClauseMetadata::new(
            field.name(),
            In,
            (start_idx..values.len() + 1).collect(),
        ));

        for val in values {
            let field_value: F::Type = val.into();
            self.where_params.push(field_value.into());
        }
        self
    }

    pub fn where_field_null<F>(mut self, field: F) -> Self
    where
        F: Field<T>,
    {
        self.where_conditions
            .push(format!("{} IS NULL", field.name()));
        self.where_metadata
            .push(WhereClauseMetadata::new(field.name(), IsNull, vec![]));
        self
    }

    pub fn where_field_not_null<F>(mut self, field: F) -> Self
    where
        F: Field<T>,
    {
        self.where_conditions
            .push(format!("{} IS NOT NULL", field.name()));
        self.where_metadata
            .push(WhereClauseMetadata::new(field.name(), IsNotNull, vec![]));
        self
    }

    pub fn where_field_between<F, V>(mut self, column: F, start: V, end: V) -> Self
    where
        F: Field<T>,
        V: Into<F::Type>,
    {
        let idx1 = self.where_params.len() + 1;
        let idx2 = idx1 + 1;
        let ph1 = self.extract_placeholder(idx1);
        let ph2 = self.extract_placeholder(idx2);

        self.where_conditions
            .push(format!("{} BETWEEN {} AND {}", column.name(), ph1, ph2));
        self.where_metadata.push(WhereClauseMetadata::new(
            column.name(),
            Between,
            vec![idx1, idx2],
        ));
        self.where_params.push(start.into().into());
        self.where_params.push(end.into().into());
        self
    }

    pub fn order_by_field<F>(mut self, field: F) -> Self
    where
        F: Orderable<T>,
    {
        self.order_by.push(format!("{} ASC", field.name()));
        self
    }

    pub fn order_by_field_desc<F>(mut self, field: F) -> Self
    where
        F: Orderable<T>,
    {
        self.order_by.push(format!("{} DESC", field.name()));
        self
    }

    pub fn select_fields<F>(mut self, fields: &[F]) -> Self
    where
        F: Field<T>,
    {
        self.select_cols = fields.iter().map(|f| f.name().to_string()).collect();
        self
    }
}

// ============================================================================
// MIGRATION SYSTEM
// ============================================================================

#[derive(Clone, Debug)]
pub struct Migration {
    pub version: i64,
    pub name: String,
    pub up: String,
    pub down: String,
}

impl Migration {
    pub fn new(version: i64, name: &str, up: &str, down: &str) -> Self {
        Self {
            version,
            name: name.to_string(),
            up: up.to_string(),
            down: down.to_string(),
        }
    }
}

pub struct MigrationRunner<'a> {
    db: &'a Flux,
}

impl<'a> MigrationRunner<'a> {
    pub fn new(db: &'a Flux) -> Self {
        Self { db }
    }

    pub async fn run(&self, migrations: &[Migration]) -> Result<()> {
        self.ensure_migrations_table().await?;

        let applied = self.get_applied_migrations().await?;

        for migration in migrations {
            if !applied.contains(&migration.version) {
                self.apply_migration(migration).await?;
            }
        }

        Ok(())
    }

    pub async fn rollback(&self, migrations: &[Migration], steps: usize) -> Result<()> {
        let mut applied = self.get_applied_migrations().await?;
        applied.sort_by(|a, b| b.cmp(a));

        for version in applied.iter().take(steps) {
            if let Some(migration) = migrations.iter().find(|m| m.version == *version) {
                self.rollback_migration(migration).await.map_err(|e| {
                    FluxError::Migration(format!(
                        "Failed to rollback migration {}: {}",
                        migration.name, e
                    ))
                })?;
            }
        }

        Ok(())
    }

    pub async fn status(&self) -> Result<Vec<MigrationStatus>> {
        self.ensure_migrations_table().await?;

        let sql = "SELECT version, name, applied_at FROM flux_migrations ORDER BY version";
        let rows = self.db.backend.fetch_all(sql, &[]).await?;

        Ok(rows
            .into_iter()
            .map(|row| MigrationStatus {
                version: row.get("version").and_then(|v| v.as_i64()).unwrap_or(0),
                name: row
                    .get("name")
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
                applied_at: row.get("applied_at").and_then(|v| v.as_datetime()),
            })
            .collect())
    }

    async fn ensure_migrations_table(&self) -> Result<()> {
        let sql = match self.db.backend.dialect() {
            Dialect::SQLite => {
                "CREATE TABLE IF NOT EXISTS flux_migrations (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    version INTEGER NOT NULL UNIQUE,
                    name TEXT NOT NULL,
                    applied_at DATETIME NOT NULL
                )"
            }
            Dialect::PostgreSQL => {
                "CREATE TABLE IF NOT EXISTS flux_migrations (
                    id SERIAL PRIMARY KEY,
                    version BIGINT NOT NULL UNIQUE,
                    name TEXT NOT NULL,
                    applied_at TIMESTAMP WITH TIME ZONE NOT NULL
                )"
            }
            Dialect::MySQL => {
                "CREATE TABLE IF NOT EXISTS flux_migrations (
                    id INT AUTO_INCREMENT PRIMARY KEY,
                    version BIGINT NOT NULL UNIQUE,
                    name VARCHAR(255) NOT NULL,
                    applied_at DATETIME NOT NULL
                )"
            }
        };

        self.db.backend.execute(sql, &[]).await?;
        Ok(())
    }

    async fn get_applied_migrations(&self) -> Result<Vec<i64>> {
        let sql = "SELECT version FROM flux_migrations";
        let rows = self.db.backend.fetch_all(sql, &[]).await?;

        Ok(rows
            .iter()
            .filter_map(|row| row.get("version").and_then(|v| v.as_i64()))
            .collect())
    }

    async fn apply_migration(&self, migration: &Migration) -> Result<()> {
        let mut tx = self.db.backend.begin_transaction().await?;

        tx.execute(&migration.up, &[]).await?;

        let sql = format!(
            "INSERT INTO flux_migrations (version, name, applied_at) VALUES ({}, {}, {})",
            tx.dialect().placeholder(1),
            tx.dialect().placeholder(2),
            tx.dialect().placeholder(3)
        );

        tx.execute(
            &sql,
            &[
                Value::I64(migration.version),
                Value::String(migration.name.clone()),
                Value::String(Utc::now().to_rfc3339()),
            ],
        )
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn rollback_migration(&self, migration: &Migration) -> Result<()> {
        let mut tx = self.db.backend.begin_transaction().await?;

        tx.execute(&migration.down, &[]).await?;

        let sql = format!(
            "DELETE FROM flux_migrations WHERE version = {}",
            tx.dialect().placeholder(1)
        );

        tx.execute(&sql, &[Value::I64(migration.version)]).await?;

        tx.commit().await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MigrationStatus {
    pub version: i64,
    pub name: String,
    pub applied_at: Option<DateTime<Utc>>,
}
