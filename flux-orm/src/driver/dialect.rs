#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    SQLite,
    PostgreSQL,
    MySQL,
    MSSQL
}

impl Dialect {
    pub fn get_placeholder(&self) -> &'static str {
        match self {
            Dialect::PostgreSQL => "$",
            Dialect::MySQL | Dialect::SQLite | Dialect::MSSQL => "?",
        }
    }
    pub fn placeholder(&self, index: usize) -> String {
        match self {
            Dialect::PostgreSQL => format!("${}", index),
            Dialect::MySQL | Dialect::SQLite | Dialect::MSSQL => "?".to_string(),
        }
    }

    pub fn quote_identifier(&self, ident: &str) -> String {
        match self {
            Dialect::PostgreSQL => format!("\"{}\"", ident.replace('\"', "\"\"")),
            Dialect::MySQL | Dialect::MSSQL => format!("`{}`", ident.replace('`', "``")),
            Dialect::SQLite => format!("\"{}\"", ident.replace('\"', "\"\"")),
        }
    }

    pub fn returning_clause(&self) -> &str {
        match self {
            Dialect::PostgreSQL => " RETURNING *",
            Dialect::MySQL | Dialect::SQLite | Dialect::MSSQL => "",
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