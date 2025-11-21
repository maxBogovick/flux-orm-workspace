use crate::driver::dialect::Dialect;

#[derive(Clone)]
pub struct WhereClauseMetadata {
    field: String,
    operation: Operator,
    indexes: Vec<usize>,
}

impl WhereClauseMetadata {
    pub(crate) fn new(field: &str, operation: Operator, indexes: Vec<usize>) -> Self {
        Self {
            field: field.to_string(),
            operation,
            indexes,
        }
    }

    pub(crate) fn new_with_default(field: &str, operation: Operator) -> Self {
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

#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
    Equals,
    NotEquals,
    GreaterThan,
    LessThan,
    GreaterThanOrEquals,
    LessThanOrEquals,
    In,
    NotIn,
    Between,
    Like,
    NotLike,
    IsNull,
    IsNotNull,
}

impl Operator {
    pub fn template(&self) -> &'static str {
        match self {
            Operator::Equals => "{} = {}",
            Operator::NotEquals => "{} != {}",
            Operator::GreaterThan => "{} > {}",
            Operator::LessThan => "{} < {}",
            Operator::GreaterThanOrEquals => "{} >= {}",
            Operator::LessThanOrEquals => "{} <= {}",
            Operator::In => "{} IN ({})",
            Operator::NotIn => "{} NOT IN ({})",
            Operator::Between => "{} BETWEEN {} AND {}",
            Operator::Like => "{} LIKE {}",
            Operator::NotLike => "{} NOT LIKE {}",
            Operator::IsNull => "{} IS NULL",
            Operator::IsNotNull => "{} IS NOT NULL",
        }
    }
}
