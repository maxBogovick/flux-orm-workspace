use crate::Value;

pub fn bind_params_sqlite<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    params: &'q [Value],
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    let mut query = query;
    for param in params {
        query = match param {
            Value::Null => query.bind(None::<i64>),
            Value::Bool(b) => query.bind(*b),
            Value::I16(i) => query.bind(*i),
            Value::I32(i) => query.bind(*i),
            Value::I64(i) => query.bind(*i),
            Value::F32(f) => query.bind(*f),
            Value::F64(f) => query.bind(*f),
            Value::String(s) => query.bind(s.as_str()),
            Value::Bytes(b) => query.bind(b.as_slice()),
            Value::DateTime(dt) => query.bind(*dt),
            Value::Uuid(u) => query.bind(u.to_string()),
            Value::Json(j) => query.bind(j.to_string()),
            #[cfg(feature = "rust_decimal")]
            Value::Decimal(d) => query.bind(d.to_string()),
            #[cfg(not(feature = "rust_decimal"))]
            Value::DecimalString(s) => query.bind(s.as_str()),
            Value::Array(arr) => {
                // Для SQLite массивы можно сериализовать в JSON-строку
                let json = serde_json::to_string(arr).unwrap();
                query.bind(json)
            }
            Value::Enum(s) => query.bind(s.as_str()),
        };
    }
    query
}

pub fn bind_params_pg<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    params: &'q [Value],
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    let mut query = query;
    for param in params {
        query = match param {
            Value::Null => query.bind(None::<i64>),
            Value::Bool(b) => query.bind(*b),
            Value::I16(i) => query.bind(*i),
            Value::I32(i) => query.bind(*i),
            Value::I64(i) => query.bind(*i),
            Value::F32(f) => query.bind(*f),
            Value::F64(f) => query.bind(*f),
            Value::String(s) => query.bind(s.as_str()),
            Value::Bytes(b) => query.bind(b.as_slice()),
            Value::DateTime(dt) => query.bind(*dt),
            Value::Uuid(u) => query.bind(*u),
            Value::Json(j) => query.bind(j),
            #[cfg(feature = "rust_decimal")]
            Value::Decimal(d) => query.bind(d),
            #[cfg(not(feature = "rust_decimal"))]
            Value::DecimalString(s) => query.bind(s.as_str()),
            Value::Array(arr) => {
                // Для PostgreSQL можно использовать массив напрямую
                let arr: Vec<serde_json::Value> = arr.iter().map(|v| match v {
                    Value::Json(j) => j.clone(),
                    _ => serde_json::to_value(v).unwrap(),
                }).collect();
                query.bind(arr)
            }
            Value::Enum(s) => query.bind(s.as_str()),
        };
    }
    query
}

pub fn bind_params_mysql<'q>(
    query: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    params: &'q [Value],
) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    let mut query = query;
    for param in params {
        query = match param {
            Value::Null => query.bind(None::<i64>),
            Value::Bool(b) => query.bind(*b),
            Value::I16(i) => query.bind(*i),
            Value::I32(i) => query.bind(*i),
            Value::I64(i) => query.bind(*i),
            Value::F32(f) => query.bind(*f),
            Value::F64(f) => query.bind(*f),
            Value::String(s) => query.bind(s.as_str()),
            Value::Bytes(b) => query.bind(b.as_slice()),
            Value::DateTime(dt) => query.bind(*dt),
            Value::Uuid(u) => query.bind(u.to_string()),
            Value::Json(j) => query.bind(j.to_string()),
            #[cfg(feature = "rust_decimal")]
            Value::Decimal(d) => query.bind(d.to_string()),
            #[cfg(not(feature = "rust_decimal"))]
            Value::DecimalString(s) => query.bind(s.as_str()),
            Value::Array(arr) => {
                // Для MySQL массивы можно сериализовать в JSON-строку
                let json = serde_json::to_string(arr).unwrap();
                query.bind(json)
            }
            Value::Enum(s) => query.bind(s.as_str()),
        };
    }
    query
}
