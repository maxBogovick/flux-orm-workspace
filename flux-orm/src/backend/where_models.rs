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