use std::any::Any;
use std::collections::HashMap;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crate::backend::common_models::Value;
use crate::backend::errors::FluxError;
use crate::query::builder::Query;
use crate::Flux;

#[async_trait]
pub trait Model: Sized + Send + Sync + Clone + Any + 'static {
    const TABLE: &'static str;
    const PRIMARY_KEY: &'static str = "id";

    type Id: Clone + Send + Sync + Into<Value> + TryFrom<Value, Error = FluxError>;

    fn id(&self) -> Option<Self::Id>;
    fn set_id(&mut self, id: Self::Id);
    fn to_values(&self) -> HashMap<String, Value>;
    fn from_values(values: HashMap<String, Value>) -> crate::backend::errors::Result<Self>;

    fn validate(&self) -> crate::backend::errors::Result<()> {
        Ok(())
    }

    async fn before_create(&mut self, _db: &Flux) -> crate::backend::errors::Result<()> {
        Ok(())
    }
    async fn after_create(&self, _db: &Flux) -> crate::backend::errors::Result<()> {
        Ok(())
    }
    async fn before_update(&mut self, _db: &Flux) -> crate::backend::errors::Result<()> {
        Ok(())
    }
    async fn after_update(&self, _db: &Flux) -> crate::backend::errors::Result<()> {
        Ok(())
    }
    async fn before_delete(&self, _db: &Flux) -> crate::backend::errors::Result<()> {
        Ok(())
    }
    async fn after_delete(&self, _db: &Flux) -> crate::backend::errors::Result<()> {
        Ok(())
    }
}

#[async_trait]
pub trait HasMany<T: Model>: Model {
    fn foreign_key() -> &'static str;

    async fn load_many(&self, db: &Flux) -> crate::backend::errors::Result<Vec<T>> {
        let id = self.id().ok_or(FluxError::NoId)?;
        db.query(Query::<T>::new().where_eq(Self::foreign_key(), id))
            .await
    }
}

#[async_trait]
pub trait HasOne<T: Model>: Model {
    fn foreign_key() -> &'static str;

    async fn load_one(&self, db: &Flux) -> crate::backend::errors::Result<Option<T>> {
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

    async fn load_parent(&self, db: &Flux) -> crate::backend::errors::Result<Option<T>> {
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

    async fn load_many(&self, db: &Flux) -> crate::backend::errors::Result<Vec<T>> {
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

    async fn attach(&self, db: &Flux, related_id: T::Id) -> crate::backend::errors::Result<()> {
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

    async fn detach(&self, db: &Flux, related_id: T::Id) -> crate::backend::errors::Result<()> {
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

    async fn soft_delete(&mut self, db: &Flux) -> crate::backend::errors::Result<()> {
        self.set_deleted_at(Some(Utc::now()));
        self.before_delete(db).await?;
        db.update(self.clone()).await?;
        self.after_delete(db).await
    }

    async fn restore(&mut self, db: &Flux) -> crate::backend::errors::Result<()> {
        self.set_deleted_at(None);
        db.update(self.clone()).await
    }

    async fn force_delete(self, db: &Flux) -> crate::backend::errors::Result<()> {
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