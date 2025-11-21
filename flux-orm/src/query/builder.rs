use crate::backend::common_models::Value;
use crate::query::condition::Operator::{Between, Equals, GreaterThan, GreaterThanOrEquals, In, IsNotNull, IsNull, LessThan, LessThanOrEquals, Like, NotEquals, NotIn};
use crate::query::condition::{Operator, WhereClauseMetadata};
use std::marker::PhantomData;
use crate::core::model::{Field, Model, Orderable};
use crate::driver::dialect::Dialect;

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

impl<T: Model> Query<T> {
    pub fn where_conditions(&self) -> &Vec<String> {
        &self.where_conditions
    }
    
    pub fn where_conditions_mut(&mut self) -> &mut Vec<String> {
        &mut self.where_conditions
    }

    pub fn table(&self) -> &str {
        &self.table
    }

    pub fn where_metadata(&self) -> &Vec<WhereClauseMetadata> {
        &self.where_metadata
    }

    pub fn where_metadata_mut(&mut self) -> &mut Vec<WhereClauseMetadata> {
        &mut self.where_metadata
    }

    pub fn where_params(&self) -> &Vec<Value> {
        &self.where_params
    }

    pub fn where_params_mut(&mut self) -> &mut Vec<Value> {
        &mut self.where_params
    }

    pub fn order_by_mut(&mut self) -> &mut Vec<String> {
        &mut self.order_by
    }

    pub fn select_cols(&self) -> &Vec<String> {
        &self.select_cols
    }
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
        where_eq => Equals,
        where_ne => NotEquals,
        where_gt => GreaterThan,
        where_gte => GreaterThanOrEquals,
        where_lt => LessThan,
        where_lte => LessThanOrEquals,
        where_like => Like,
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

    pub(crate) fn extract_placeholder(&self, idx: usize) -> String {
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