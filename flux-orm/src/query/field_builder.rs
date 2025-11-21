use crate::backend::common_models::Value;
use crate::core::model::Model;
use crate::query::builder::Query;
use crate::query::condition::{Operator, WhereClauseMetadata};
use std::marker::PhantomData;

/// Билдер для типобезопасного создания условий WHERE
pub struct FieldConditionBuilder<T: Model> {
    field_name: &'static str,
    operator: Operator,
    values: Vec<Value>,
    _marker: PhantomData<T>,
}

impl<T: Model> FieldConditionBuilder<T> {
    pub fn new(field_name: &'static str, operator: Operator, values: Vec<Value>) -> Self {
        Self {
            field_name,
            operator,
            values,
            _marker: PhantomData,
        }
    }

    pub fn field_name(&self) -> &'static str {
        self.field_name
    }

    pub fn operator(&self) -> &Operator {
        &self.operator
    }

    pub fn values(&self) -> &[Value] {
        &self.values
    }

    /// Применяет условие к Query
    pub fn apply_to(self, mut query: Query<T>) -> Query<T> {
        match self.operator {
            Operator::Equals => {
                query = query.where_field_eq_internal(self.field_name, self.values[0].clone());
            }
            Operator::NotEquals => {
                query = query.where_field_ne_internal(self.field_name, self.values[0].clone());
            }
            Operator::GreaterThan => {
                query = query.where_field_gt_internal(self.field_name, self.values[0].clone());
            }
            Operator::GreaterThanOrEquals => {
                query = query.where_field_gte_internal(self.field_name, self.values[0].clone());
            }
            Operator::LessThan => {
                query = query.where_field_lt_internal(self.field_name, self.values[0].clone());
            }
            Operator::LessThanOrEquals => {
                query = query.where_field_lte_internal(self.field_name, self.values[0].clone());
            }
            Operator::Like => {
                query = query.where_field_like_internal(self.field_name, self.values[0].clone());
            }
            Operator::NotLike => {
                query =
                    query.where_field_not_like_internal(self.field_name, self.values[0].clone());
            }
            Operator::In => {
                query = query.where_field_in_internal(self.field_name, self.values);
            }
            Operator::NotIn => {
                query = query.where_field_not_in_internal(self.field_name, self.values);
            }
            Operator::Between => {
                if self.values.len() >= 2 {
                    query = query.where_field_between_internal(
                        self.field_name,
                        self.values[0].clone(),
                        self.values[1].clone(),
                    );
                }
            }
            Operator::IsNull => {
                query = query.where_field_null_internal(self.field_name);
            }
            Operator::IsNotNull => {
                query = query.where_field_not_null_internal(self.field_name);
            }
        }
        query
    }
}

/// Структура для сортировки
pub struct FieldOrder<T: Model> {
    pub field_name: &'static str,
    pub descending: bool,
    pub _marker: PhantomData<T>,
}

// Добавьте в impl<T: Model> Query<T> внутренние методы:
impl<T: Model> Query<T> {
    // Внутренние методы для применения условий
    fn where_field_eq_internal(mut self, field: &str, value: Value) -> Self {
        let idx = self.where_params().len() + 1;
        let placeholder = self.extract_placeholder(idx);

        self.where_conditions_mut()
            .push(format!("{} = {}", field, placeholder));
        self.where_metadata_mut().push(WhereClauseMetadata::new(
            field,
            Operator::Equals,
            vec![idx],
        ));
        self.where_params_mut().push(value);
        self
    }

    fn where_field_ne_internal(mut self, field: &str, value: Value) -> Self {
        let idx = self.where_params().len() + 1;
        let placeholder = self.extract_placeholder(idx);

        self.where_conditions_mut()
            .push(format!("{} != {}", field, placeholder));
        self.where_metadata_mut().push(WhereClauseMetadata::new(
            field,
            Operator::NotEquals,
            vec![idx],
        ));
        self.where_params_mut().push(value);
        self
    }

    fn where_field_gt_internal(mut self, field: &str, value: Value) -> Self {
        let idx = self.where_params().len() + 1;
        let placeholder = self.extract_placeholder(idx);

        self.where_conditions_mut()
            .push(format!("{} > {}", field, placeholder));
        self.where_metadata_mut().push(WhereClauseMetadata::new(
            field,
            Operator::GreaterThan,
            vec![idx],
        ));
        self.where_params_mut().push(value);
        self
    }

    fn where_field_gte_internal(mut self, field: &str, value: Value) -> Self {
        let idx = self.where_params().len() + 1;
        let placeholder = self.extract_placeholder(idx);

        self.where_conditions_mut()
            .push(format!("{} >= {}", field, placeholder));
        self.where_metadata_mut().push(WhereClauseMetadata::new(
            field,
            Operator::GreaterThanOrEquals,
            vec![idx],
        ));
        self.where_params_mut().push(value);
        self
    }

    fn where_field_lt_internal(mut self, field: &str, value: Value) -> Self {
        let idx = self.where_params().len() + 1;
        let placeholder = self.extract_placeholder(idx);

        self.where_conditions_mut()
            .push(format!("{} < {}", field, placeholder));
        self.where_metadata_mut().push(WhereClauseMetadata::new(
            field,
            Operator::LessThan,
            vec![idx],
        ));
        self.where_params_mut().push(value);
        self
    }

    fn where_field_lte_internal(mut self, field: &str, value: Value) -> Self {
        let idx = self.where_params().len() + 1;
        let placeholder = self.extract_placeholder(idx);

        self.where_conditions_mut()
            .push(format!("{} <= {}", field, placeholder));
        self.where_metadata_mut().push(WhereClauseMetadata::new(
            field,
            Operator::LessThanOrEquals,
            vec![idx],
        ));
        self.where_params_mut().push(value);
        self
    }

    fn where_field_like_internal(mut self, field: &str, value: Value) -> Self {
        let idx = self.where_params().len() + 1;
        let placeholder = self.extract_placeholder(idx);

        self.where_conditions_mut()
            .push(format!("{} LIKE {}", field, placeholder));
        self.where_metadata_mut()
            .push(WhereClauseMetadata::new(field, Operator::Like, vec![idx]));
        self.where_params_mut().push(value);
        self
    }

    fn where_field_not_like_internal(mut self, field: &str, value: Value) -> Self {
        let idx = self.where_params().len() + 1;
        let placeholder = self.extract_placeholder(idx);

        self.where_conditions_mut()
            .push(format!("{} NOT LIKE {}", field, placeholder));
        self.where_metadata_mut().push(WhereClauseMetadata::new(
            field,
            Operator::NotLike,
            vec![idx],
        ));
        self.where_params_mut().push(value);
        self
    }

    fn where_field_in_internal(mut self, field: &str, values: Vec<Value>) -> Self {
        if values.is_empty() {
            return self;
        }

        let start_idx = self.where_params().len() + 1;
        let placeholders: Vec<String> = (0..values.len())
            .map(|i| self.extract_placeholder(start_idx + i))
            .collect();

        self.where_conditions_mut()
            .push(format!("{} IN ({})", field, placeholders.join(", ")));
        self.where_metadata_mut().push(WhereClauseMetadata::new(
            field,
            Operator::In,
            (start_idx..start_idx + values.len()).collect(),
        ));

        for val in values {
            self.where_params_mut().push(val);
        }
        self
    }

    fn where_field_not_in_internal(mut self, field: &str, values: Vec<Value>) -> Self {
        if values.is_empty() {
            return self;
        }

        let start_idx = self.where_params().len() + 1;
        let placeholders: Vec<String> = (0..values.len())
            .map(|i| self.extract_placeholder(start_idx + i))
            .collect();

        self.where_conditions_mut()
            .push(format!("{} NOT IN ({})", field, placeholders.join(", ")));
        self.where_metadata_mut().push(WhereClauseMetadata::new(
            field,
            Operator::NotIn,
            (start_idx..start_idx + values.len()).collect(),
        ));

        for val in values {
            self.where_params_mut().push(val);
        }
        self
    }

    fn where_field_between_internal(mut self, field: &str, start: Value, end: Value) -> Self {
        let idx1 = self.where_params().len() + 1;
        let idx2 = idx1 + 1;
        let ph1 = self.extract_placeholder(idx1);
        let ph2 = self.extract_placeholder(idx2);

        self.where_conditions_mut()
            .push(format!("{} BETWEEN {} AND {}", field, ph1, ph2));
        self.where_metadata_mut().push(WhereClauseMetadata::new(
            field,
            Operator::Between,
            vec![idx1, idx2],
        ));
        self.where_params_mut().push(start);
        self.where_params_mut().push(end);
        self
    }

    fn where_field_null_internal(mut self, field: &str) -> Self {
        self.where_conditions_mut()
            .push(format!("{} IS NULL", field));
        self.where_metadata_mut()
            .push(WhereClauseMetadata::new_with_default(
                field,
                Operator::IsNull,
            ));
        self
    }

    fn where_field_not_null_internal(mut self, field: &str) -> Self {
        self.where_conditions_mut()
            .push(format!("{} IS NOT NULL", field));
        self.where_metadata_mut()
            .push(WhereClauseMetadata::new_with_default(
                field,
                Operator::IsNotNull,
            ));
        self
    }

    /// Публичный метод для применения FieldConditionBuilder
    pub fn where_condition(self, condition: FieldConditionBuilder<T>) -> Self {
        condition.apply_to(self)
    }

    /// Публичный метод для применения сортировки
    pub fn order_by_condition(mut self, order: FieldOrder<T>) -> Self {
        let direction = if order.descending { "DESC" } else { "ASC" };
        self.order_by_mut()
            .push(format!("{} {}", order.field_name, direction));
        self
    }
}
