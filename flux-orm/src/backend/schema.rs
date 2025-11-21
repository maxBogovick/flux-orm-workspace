use crate::driver::dialect::Dialect;

/// Схема таблицы базы данных
#[derive(Debug, Clone)]
pub struct TableSchema {
    pub table_name: String,
    pub columns: Vec<ColumnDefinition>,
    pub has_timestamps: bool,
    pub has_soft_delete: bool,
}

/// Определение колонки
#[derive(Debug, Clone)]
pub struct ColumnDefinition {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub indexed: bool,
    pub auto_increment: bool,
    pub max_length: Option<usize>,
    pub default: Option<String>,
}

/// Трейт для работы со схемой модели
pub trait Schema {
    /// Получить схему таблицы
    fn table_schema() -> TableSchema;

    /// Создать SQL для создания таблицы
    fn create_table_sql(dialect: Dialect) -> String;

    /// Создать SQL для удаления таблицы
    fn drop_table_sql() -> String;

    /// Создать SQL для добавления колонки
    fn add_column_sql(column: &ColumnDefinition, dialect: Dialect) -> String;

    /// Создать SQL для удаления колонки
    fn drop_column_sql(column_name: &str) -> String;

    /// Создать SQL для создания индекса
    fn create_index_sql(column_name: &str, index_name: Option<&str>) -> String;
}

impl TableSchema {
    /// Конвертировать схему в SQL для создания таблицы
    pub fn to_create_table_sql(&self, dialect: Dialect) -> String {
        let mut sql = format!("CREATE TABLE {} (\n", self.table_name);

        let column_defs: Vec<String> = self
            .columns
            .iter()
            .map(|col| format!("  {}", col.to_sql(dialect)))
            .collect();

        sql.push_str(&column_defs.join(",\n"));

        // Добавляем timestamps если нужно
        if self.has_timestamps {
            sql.push_str(",\n");
            sql.push_str(&format!(
                "  created_at {} NOT NULL DEFAULT {}",
                match dialect {
                    Dialect::PostgreSQL => "TIMESTAMP",
                    Dialect::MySQL => "TIMESTAMP",
                    Dialect::SQLite => "DATETIME",
                    Dialect::MSSQL => "DATETIME2",
                },
                match dialect {
                    Dialect::PostgreSQL => "CURRENT_TIMESTAMP",
                    Dialect::MySQL => "CURRENT_TIMESTAMP",
                    Dialect::SQLite => "CURRENT_TIMESTAMP",
                    Dialect::MSSQL => "GETDATE()",
                }
            ));
            sql.push_str(",\n");
            sql.push_str(&format!(
                "  updated_at {} NOT NULL DEFAULT {}",
                match dialect {
                    Dialect::PostgreSQL => "TIMESTAMP",
                    Dialect::MySQL => "TIMESTAMP",
                    Dialect::SQLite => "DATETIME",
                    Dialect::MSSQL => "DATETIME2",
                },
                match dialect {
                    Dialect::PostgreSQL => "CURRENT_TIMESTAMP",
                    Dialect::MySQL => "CURRENT_TIMESTAMP",
                    Dialect::SQLite => "CURRENT_TIMESTAMP",
                    Dialect::MSSQL => "GETDATE()",
                }
            ));
        }

        // Добавляем soft delete если нужно
        if self.has_soft_delete {
            sql.push_str(",\n");
            sql.push_str(&format!(
                "  deleted_at {}",
                match dialect {
                    Dialect::PostgreSQL => "TIMESTAMP",
                    Dialect::MySQL => "TIMESTAMP",
                    Dialect::SQLite => "DATETIME",
                    Dialect::MSSQL => "DATETIME2",
                }
            ));
        }

        sql.push_str("\n)");

        // Добавляем engine для MySQL
        if dialect == Dialect::MySQL {
            sql.push_str(" ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci");
        }

        sql.push(';');
        sql
    }

    /// Получить SQL для создания индексов
    pub fn create_indexes_sql(&self, dialect: Dialect) -> Vec<String> {
        let mut indexes = Vec::new();

        for col in &self.columns {
            if col.indexed && !col.primary_key {
                let index_name = format!("idx_{}_{}", self.table_name, col.name);
                indexes.push(format!(
                    "CREATE INDEX {} ON {} ({});",
                    index_name, self.table_name, col.name
                ));
            }

            if col.unique && !col.primary_key {
                let index_name = format!("uq_{}_{}", self.table_name, col.name);
                indexes.push(match dialect {
                    Dialect::PostgreSQL | Dialect::SQLite => {
                        format!(
                            "CREATE UNIQUE INDEX {} ON {} ({});",
                            index_name, self.table_name, col.name
                        )
                    }
                    Dialect::MySQL | Dialect::MSSQL => {
                        format!(
                            "ALTER TABLE {} ADD CONSTRAINT {} UNIQUE ({});",
                            self.table_name, index_name, col.name
                        )
                    }
                });
            }
        }

        indexes
    }
}

impl ColumnDefinition {
    /// Конвертировать определение колонки в SQL
    pub fn to_sql(&self, dialect: Dialect) -> String {
        let mut sql = format!("{} {}", self.name, self.map_type_to_dialect(dialect));

        // Добавляем ограничения
        if self.primary_key {
            sql.push_str(" PRIMARY KEY");

            if self.auto_increment {
                sql.push_str(match dialect {
                    Dialect::PostgreSQL => " GENERATED ALWAYS AS IDENTITY",
                    Dialect::MySQL => " AUTO_INCREMENT",
                    Dialect::SQLite => " AUTOINCREMENT",
                    Dialect::MSSQL => " IDENTITY(1,1)",
                });
            }
        }

        if !self.nullable && !self.primary_key {
            sql.push_str(" NOT NULL");
        }

        if let Some(ref default) = self.default {
            sql.push_str(&format!(" DEFAULT {}", default));
        }

        sql
    }

    /// Маппинг типа в зависимости от диалекта
    fn map_type_to_dialect(&self, dialect: Dialect) -> String {
        let base_type = self.sql_type.as_str();

        match dialect {
            Dialect::PostgreSQL => match base_type {
                "INTEGER" => "INTEGER".to_string(),
                "BIGINT" => "BIGINT".to_string(),
                "SMALLINT" => "SMALLINT".to_string(),
                "REAL" => "REAL".to_string(),
                "DOUBLE PRECISION" => "DOUBLE PRECISION".to_string(),
                "BOOLEAN" => "BOOLEAN".to_string(),
                "TEXT" => {
                    if let Some(len) = self.max_length {
                        format!("VARCHAR({})", len)
                    } else {
                        "TEXT".to_string()
                    }
                }
                "TIMESTAMP" => "TIMESTAMP".to_string(),
                "UUID" => "UUID".to_string(),
                "JSONB" => "JSONB".to_string(),
                _ => base_type.to_string(),
            },
            Dialect::MySQL => match base_type {
                "INTEGER" => "INT".to_string(),
                "BIGINT" => "BIGINT".to_string(),
                "SMALLINT" => "SMALLINT".to_string(),
                "REAL" => "FLOAT".to_string(),
                "DOUBLE PRECISION" => "DOUBLE".to_string(),
                "BOOLEAN" => "TINYINT(1)".to_string(),
                "TEXT" => {
                    if let Some(len) = self.max_length {
                        if len <= 255 {
                            format!("VARCHAR({})", len)
                        } else if len <= 65535 {
                            "TEXT".to_string()
                        } else {
                            "LONGTEXT".to_string()
                        }
                    } else {
                        "TEXT".to_string()
                    }
                }
                "TIMESTAMP" => "TIMESTAMP".to_string(),
                "UUID" => "CHAR(36)".to_string(),
                "JSONB" => "JSON".to_string(),
                _ => base_type.to_string(),
            },
            Dialect::SQLite => match base_type {
                "INTEGER" | "BIGINT" | "SMALLINT" => "INTEGER".to_string(),
                "REAL" | "DOUBLE PRECISION" => "REAL".to_string(),
                "BOOLEAN" => "INTEGER".to_string(),
                "TEXT" => "TEXT".to_string(),
                "TIMESTAMP" => "DATETIME".to_string(),
                "UUID" => "TEXT".to_string(),
                "JSONB" => "TEXT".to_string(),
                _ => "TEXT".to_string(),
            },
            Dialect::MSSQL => match base_type {
                "INTEGER" => "INT".to_string(),
                "BIGINT" => "BIGINT".to_string(),
                "SMALLINT" => "SMALLINT".to_string(),
                "REAL" => "REAL".to_string(),
                "DOUBLE PRECISION" => "FLOAT".to_string(),
                "BOOLEAN" => "BIT".to_string(),
                "TEXT" => {
                    if let Some(len) = self.max_length {
                        if len <= 8000 {
                            format!("NVARCHAR({})", len)
                        } else {
                            "NVARCHAR(MAX)".to_string()
                        }
                    } else {
                        "NVARCHAR(MAX)".to_string()
                    }
                }
                "TIMESTAMP" => "DATETIME2".to_string(),
                "UUID" => "UNIQUEIDENTIFIER".to_string(),
                "JSONB" => "NVARCHAR(MAX)".to_string(),
                _ => base_type.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_schema_postgresql() {
        let schema = TableSchema {
            table_name: "users".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    sql_type: "BIGINT".to_string(),
                    nullable: false,
                    primary_key: true,
                    unique: false,
                    indexed: false,
                    auto_increment: true,
                    max_length: None,
                    default: None,
                },
                ColumnDefinition {
                    name: "email".to_string(),
                    sql_type: "TEXT".to_string(),
                    nullable: false,
                    primary_key: false,
                    unique: true,
                    indexed: false,
                    auto_increment: false,
                    max_length: Some(255),
                    default: None,
                },
            ],
            has_timestamps: true,
            has_soft_delete: false,
        };

        let sql = schema.to_create_table_sql(Dialect::PostgreSQL);
        assert!(sql.contains("CREATE TABLE users"));
        assert!(sql.contains("id BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY"));
        assert!(sql.contains("email VARCHAR(255) NOT NULL"));
        assert!(sql.contains("created_at TIMESTAMP NOT NULL"));
    }

    #[test]
    fn test_table_schema_mysql() {
        let schema = TableSchema {
            table_name: "posts".to_string(),
            columns: vec![ColumnDefinition {
                name: "id".to_string(),
                sql_type: "INTEGER".to_string(),
                nullable: false,
                primary_key: true,
                unique: false,
                indexed: false,
                auto_increment: true,
                max_length: None,
                default: None,
            }],
            has_timestamps: false,
            has_soft_delete: true,
        };

        let sql = schema.to_create_table_sql(Dialect::MySQL);
        assert!(sql.contains("CREATE TABLE posts"));
        assert!(sql.contains("ENGINE=InnoDB"));
        assert!(sql.contains("deleted_at TIMESTAMP"));
    }

    #[test]
    fn test_create_indexes() {
        let schema = TableSchema {
            table_name: "products".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    sql_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    unique: false,
                    indexed: false,
                    auto_increment: true,
                    max_length: None,
                    default: None,
                },
                ColumnDefinition {
                    name: "sku".to_string(),
                    sql_type: "TEXT".to_string(),
                    nullable: false,
                    primary_key: false,
                    unique: true,
                    indexed: false,
                    auto_increment: false,
                    max_length: Some(50),
                    default: None,
                },
                ColumnDefinition {
                    name: "category_id".to_string(),
                    sql_type: "INTEGER".to_string(),
                    nullable: true,
                    primary_key: false,
                    unique: false,
                    indexed: true,
                    auto_increment: false,
                    max_length: None,
                    default: None,
                },
            ],
            has_timestamps: false,
            has_soft_delete: false,
        };

        let indexes = schema.create_indexes_sql(Dialect::PostgreSQL);
        assert_eq!(indexes.len(), 2);
        assert!(indexes[0].contains("CREATE UNIQUE INDEX"));
        assert!(indexes[1].contains("CREATE INDEX"));
    }
}