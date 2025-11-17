// src/backend/row_mapper.rs
use crate::{Result, Value};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{
    Column, Decode, Row, Type, TypeInfo,
    mysql::{MySql, MySqlRow},
    postgres::{PgRow, Postgres},
    sqlite::{Sqlite, SqliteRow},
};
use std::collections::HashMap;
use uuid::Uuid;

#[cfg(feature = "rust_decimal")]
use rust_decimal::Decimal;

// =====================================================================
// CONFIGURATION
// =====================================================================

/// Mode for decoding datetime-like values for non-Postgres DBs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateTimeMode {
    /// Convert to UTC DateTime (default)
    AsUtc,
    /// Keep as string representation
    AsString,
}

impl Default for DateTimeMode {
    fn default() -> Self {
        DateTimeMode::AsUtc
    }
}

// =====================================================================
// HELPER DECODERS (Flattened Option handling)
// =====================================================================

/// Decode PostgreSQL column, flattening Option<Option<T>> to Option<T>
fn decode_pg_flat<T>(row: &PgRow, name: &str) -> Option<T>
where
    for<'r> T: Decode<'r, Postgres> + Type<Postgres>,
{
    row.try_get::<Option<T>, _>(name).ok().flatten()
}

/// Decode MySQL column, flattening Option<Option<T>> to Option<T>
fn decode_mysql_flat<T>(row: &MySqlRow, name: &str) -> Option<T>
where
    for<'r> T: Decode<'r, MySql> + Type<MySql>,
{
    row.try_get::<Option<T>, _>(name).ok().flatten()
}

/// Decode SQLite column, flattening Option<Option<T>> to Option<T>
fn decode_sqlite_flat<T>(row: &SqliteRow, name: &str) -> Option<T>
where
    for<'r> T: Decode<'r, Sqlite> + Type<Sqlite>,
{
    row.try_get::<Option<T>, _>(name).ok().flatten()
}

// =====================================================================
// JSON CONVERSION HELPERS
// =====================================================================

/// Convert serde_json::Value to our Value type
fn json_to_value(j: JsonValue) -> Value {
    match j {
        JsonValue::String(s) => Value::String(s),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::I64(i)
            } else if let Some(u) = n.as_u64() {
                // Safe conversion for u64
                if u <= i64::MAX as u64 {
                    Value::I64(u as i64)
                } else {
                    // Store large u64 as string to avoid overflow
                    Value::String(u.to_string())
                }
            } else if let Some(f) = n.as_f64() {
                Value::F64(f)
            } else {
                unreachable!(
                    "serde_json::Number this type can not convert into any appropriated type"
                );
            }
        }
        JsonValue::Bool(b) => Value::Bool(b),
        JsonValue::Null => Value::Null,
        //TODO JsonValue::Array(arr) => Value::Array(arr.into_iter().map(json_to_value).collect()),
        other => Value::Json(other),
    }
}

// =====================================================================
// POSTGRES ARRAY DECODER
// =====================================================================

/// Try to decode PostgreSQL array columns into Vec<Value>
fn try_decode_pg_array_as_values(row: &PgRow, name: &str) -> Option<Vec<Value>> {
    // Try common array types in order of likelihood

    // Integer arrays
    if let Some(arr) = decode_pg_flat::<Vec<i64>>(row, name) {
        return Some(arr.into_iter().map(Value::I64).collect());
    }
    if let Some(arr) = decode_pg_flat::<Vec<i32>>(row, name) {
        return Some(arr.into_iter().map(Value::I32).collect());
    }
    if let Some(arr) = decode_pg_flat::<Vec<i16>>(row, name) {
        return Some(arr.into_iter().map(Value::I16).collect());
    }

    // String arrays
    if let Some(arr) = decode_pg_flat::<Vec<String>>(row, name) {
        return Some(arr.into_iter().map(Value::String).collect());
    }

    // UUID arrays
    if let Some(arr) = decode_pg_flat::<Vec<Uuid>>(row, name) {
        return Some(arr.into_iter().map(Value::Uuid).collect());
    }

    // Float arrays
    if let Some(arr) = decode_pg_flat::<Vec<f64>>(row, name) {
        return Some(arr.into_iter().map(Value::F64).collect());
    }
    if let Some(arr) = decode_pg_flat::<Vec<f32>>(row, name) {
        return Some(arr.into_iter().map(Value::F32).collect());
    }

    // Boolean arrays
    if let Some(arr) = decode_pg_flat::<Vec<bool>>(row, name) {
        return Some(arr.into_iter().map(Value::Bool).collect());
    }

    // Fallback: try as JSON array
    if let Some(JsonValue::Array(arr)) = decode_pg_flat::<JsonValue>(row, name) {
        return Some(arr.into_iter().map(json_to_value).collect());
    }

    None
}

// =====================================================================
// DECIMAL DECODING (conditional compilation)
// =====================================================================
macro_rules! define_decimal_decoder {
    (
        $fn_name:ident,
        $row_type:ty,
        $db_type:ty,
        $decode_flat_fn:ident
    ) => {
        #[cfg(feature = "rust_decimal")]
        fn $fn_name(row: &$row_type, name: &str) -> Value {
            if let Some(d) = $decode_flat_fn::<Decimal>(row, name) {
                return Value::Decimal(d);
            }
            if let Some(s) = $decode_flat_fn::<String>(row, name) {
                return s
                    .parse::<Decimal>()
                    .map(Value::Decimal)
                    .unwrap_or_else(|_| Value::String(s));
            }
            if let Some(f) = $decode_flat_fn::<f64>(row, name) {
                return Value::F64(f);
            }
            Value::Null
        }

        #[cfg(not(feature = "rust_decimal"))]
        fn $fn_name(row: &$row_type, name: &str) -> Value {
            $decode_flat_fn::<String>(row, name)
                .map(Value::String)
                .or_else(|| $decode_flat_fn::<f64>(row, name).map(Value::F64))
                .unwrap_or(Value::Null)
        }
    };
}

define_decimal_decoder!(decode_decimal_pg, PgRow, Postgres, decode_pg_flat);

define_decimal_decoder!(decode_decimal_mysql, MySqlRow, MySql, decode_mysql_flat);

define_decimal_decoder!(decode_decimal_sqlite, SqliteRow, Sqlite, decode_sqlite_flat);

// =====================================================================
// DATE/TIME HELPERS
// =====================================================================

/// Safely convert NaiveDate to DateTime<Utc> at midnight
fn naive_date_to_utc(date: NaiveDate) -> Value {
    match date.and_hms_opt(0, 0, 0) {
        Some(ndt) => Value::DateTime(ndt.and_utc()),
        None => {
            // Fallback: this should rarely happen, but handle gracefully
            Value::String(date.to_string())
        }
    }
}

/// Parse datetime from string with multiple format support
fn parse_datetime_from_string(s: String) -> Value {
    // Try RFC3339 format first
    if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
        return Value::DateTime(dt.with_timezone(&Utc));
    }

    // Try common SQL datetime formats
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
    ];

    for format in &formats {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(&s, format) {
            return Value::DateTime(ndt.and_utc());
        }
    }

    // If all parsing fails, keep as string
    Value::String(s)
}

// =====================================================================
// BOOLEAN DECODING (with fallback to integer)
// =====================================================================

/// Decode boolean with fallback to integer (0/1) handling
macro_rules! decode_bool_with_fallback {
    ($decode_fn:ident, $row:expr, $name:expr) => {{
        if let Some(b) = $decode_fn::<bool>($row, $name) {
            return Value::Bool(b);
        }
        // Fallback to integer interpretation
        match $decode_fn::<i64>($row, $name) {
            Some(0) => Value::Bool(false),
            Some(1) => Value::Bool(true),
            Some(i) => Value::I64(i),
            None => Value::Null,
        }
    }};
}

// =====================================================================
// TYPE NAME UTILITIES
// =====================================================================

/// Check if type name is likely a PostgreSQL enum or custom type
fn is_likely_enum_or_custom_type(type_name: &str) -> bool {
    let t = type_name.to_ascii_lowercase();

    // Known builtin types
    const BUILTINS: &[&str] = &[
        "int2",
        "int4",
        "int8",
        "smallint",
        "integer",
        "bigint",
        "text",
        "varchar",
        "char",
        "bpchar",
        "name",
        "bool",
        "boolean",
        "timestamp",
        "timestamptz",
        "timestamp with time zone",
        "date",
        "time",
        "json",
        "jsonb",
        "bytea",
        "numeric",
        "decimal",
        "float4",
        "float8",
        "real",
        "double precision",
        "double",
        "uuid",
    ];

    if BUILTINS.contains(&t.as_str()) {
        return false;
    }

    // Array types (end with [] or start with _)
    if t.ends_with("[]") || t.starts_with('_') {
        return false;
    }

    // Must be non-empty and contain only letters/underscores/digits
    if t.is_empty() {
        return false;
    }

    // Custom types typically have letters and may have underscores or digits
    t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// =====================================================================
// MAIN DECODERS
// =====================================================================

/// Decode a PostgreSQL column value based on its type
fn decode_value_pg(row: &PgRow, col_name: &str, type_name: &str) -> Value {
    let tn = type_name.to_ascii_uppercase();

    match tn.as_str() {
        // Boolean
        "BOOL" | "BOOLEAN" => decode_bool_with_fallback!(decode_pg_flat, row, col_name),

        // Integers
        "INT2" | "SMALLINT" => decode_pg_flat::<i16>(row, col_name)
            .map(Value::I16)
            .unwrap_or(Value::Null),
        "INT4" | "INTEGER" | "INT" => decode_pg_flat::<i32>(row, col_name)
            .map(Value::I32)
            .unwrap_or(Value::Null),
        "INT8" | "BIGINT" => decode_pg_flat::<i64>(row, col_name)
            .map(Value::I64)
            .unwrap_or(Value::Null),

        // Floats
        "FLOAT4" | "REAL" => decode_pg_flat::<f32>(row, col_name)
            .map(Value::F32)
            .unwrap_or(Value::Null),
        "FLOAT8" | "DOUBLE PRECISION" | "DOUBLE" => decode_pg_flat::<f64>(row, col_name)
            .map(Value::F64)
            .unwrap_or(Value::Null),

        // Decimal/Numeric
        "DECIMAL" | "NUMERIC" => decode_decimal_pg(row, col_name),

        // Strings
        "TEXT" | "VARCHAR" | "CHAR" | "BPCHAR" | "NAME" => decode_pg_flat::<String>(row, col_name)
            .map(Value::String)
            .unwrap_or(Value::Null),

        // Binary
        "BYTEA" => decode_pg_flat::<Vec<u8>>(row, col_name)
            .map(Value::Bytes)
            .unwrap_or(Value::Null),

        // UUID
        "UUID" => decode_pg_flat::<Uuid>(row, col_name)
            .map(Value::Uuid)
            .unwrap_or(Value::Null),

        // JSON
        "JSON" | "JSONB" => decode_pg_flat::<JsonValue>(row, col_name)
            .map(Value::Json)
            .unwrap_or(Value::Null),

        // Date
        "DATE" => decode_pg_flat::<NaiveDate>(row, col_name)
            .map(naive_date_to_utc)
            .unwrap_or(Value::Null),

        // Time
        "TIME" => decode_pg_flat::<NaiveTime>(row, col_name)
            .map(|t| Value::String(t.to_string()))
            .unwrap_or(Value::Null),

        // Timestamp
        "TIMESTAMP" | "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" => {
            // Try DateTime<Utc> first
            if let Some(dt) = decode_pg_flat::<DateTime<Utc>>(row, col_name) {
                return Value::DateTime(dt);
            }
            // Fallback to NaiveDateTime
            decode_pg_flat::<NaiveDateTime>(row, col_name)
                .map(|ndt| Value::DateTime(ndt.and_utc()))
                .unwrap_or(Value::Null)
        }

        // Arrays (PostgreSQL arrays end with [] or start with _)
        /* TODO t if t.ends_with("[]") || t.starts_with('_') => {
            if let Some(vals) = try_decode_pg_array_as_values(row, col_name) {
                Value::Array(vals)
            } else {
                // Fallback to JSON interpretation
                decode_pg_flat::<JsonValue>(row, col_name)
                    .map(|j| match j {
                        JsonValue::Array(arr) => {
                            Value::Array(arr.into_iter().map(json_to_value).collect())
                        }
                        other => Value::Json(other),
                    })
                    .unwrap_or(Value::Null)
            }
        }*/
        // Enums and custom types
        _ if is_likely_enum_or_custom_type(type_name) => decode_pg_flat::<String>(row, col_name)
            .map(Value::Enum)
            .unwrap_or(Value::Null),

        // Generic fallback
        _ => {
            // Try JSON first for unknown types
            decode_pg_flat::<JsonValue>(row, col_name)
                .map(Value::Json)
                .or_else(|| decode_pg_flat::<String>(row, col_name).map(Value::String))
                .unwrap_or(Value::Null)
        }
    }
}

/// Decode a MySQL column value based on its type
fn decode_value_mysql(
    row: &MySqlRow,
    col_name: &str,
    type_name: &str,
    mode: DateTimeMode,
) -> Value {
    let tn = type_name.to_ascii_uppercase();

    match tn.as_str() {
        // Boolean
        "TINYINT" | "BOOL" | "BOOLEAN" => {
            decode_bool_with_fallback!(decode_mysql_flat, row, col_name)
        }

        // Integers
        "SMALLINT" | "MEDIUMINT" | "INT" | "INTEGER" => decode_mysql_flat::<i32>(row, col_name)
            .map(Value::I32)
            .unwrap_or(Value::Null),
        "BIGINT" => decode_mysql_flat::<i64>(row, col_name)
            .map(Value::I64)
            .unwrap_or(Value::Null),

        // Floats
        "FLOAT" | "FLOAT4" => decode_mysql_flat::<f32>(row, col_name)
            .map(Value::F32)
            .unwrap_or(Value::Null),
        "DOUBLE" | "FLOAT8" => decode_mysql_flat::<f64>(row, col_name)
            .map(Value::F64)
            .unwrap_or(Value::Null),

        // Decimal
        "DECIMAL" | "NUMERIC" => decode_decimal_mysql(row, col_name),

        // Strings
        "VARCHAR" | "TEXT" | "CHAR" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" => {
            decode_mysql_flat::<String>(row, col_name)
                .map(Value::String)
                .unwrap_or(Value::Null)
        }

        // Binary
        "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" | "VARBINARY" | "BINARY" => {
            decode_mysql_flat::<Vec<u8>>(row, col_name)
                .map(Value::Bytes)
                .unwrap_or(Value::Null)
        }

        // UUID
        "UUID" => decode_mysql_flat::<Uuid>(row, col_name)
            .map(Value::Uuid)
            .unwrap_or(Value::Null),

        // JSON
        "JSON" => decode_mysql_flat::<JsonValue>(row, col_name)
            .map(Value::Json)
            .unwrap_or(Value::Null),

        // Date
        "DATE" => decode_mysql_flat::<NaiveDate>(row, col_name)
            .map(naive_date_to_utc)
            .unwrap_or(Value::Null),

        // Time
        "TIME" => decode_mysql_flat::<NaiveTime>(row, col_name)
            .map(|t| Value::String(t.to_string()))
            .unwrap_or(Value::Null),

        // Datetime/Timestamp
        "DATETIME" | "TIMESTAMP" => match mode {
            DateTimeMode::AsString => decode_mysql_flat::<String>(row, col_name)
                .map(Value::String)
                .unwrap_or(Value::Null),
            DateTimeMode::AsUtc => {
                // Try NaiveDateTime first
                if let Some(ndt) = decode_mysql_flat::<NaiveDateTime>(row, col_name) {
                    return Value::DateTime(ndt.and_utc());
                }
                // Fallback to string parsing
                decode_mysql_flat::<String>(row, col_name)
                    .map(parse_datetime_from_string)
                    .unwrap_or(Value::Null)
            }
        },

        // Generic fallback
        _ => {
            // Try JSON array interpretation
            if let Some(j) = decode_mysql_flat::<JsonValue>(row, col_name) {
                match j {
                    /*TODO JsonValue::Array(arr) => {
                        Value::Array(arr.into_iter().map(json_to_value).collect())
                    }*/
                    other => Value::Json(other),
                }
            } else {
                decode_mysql_flat::<String>(row, col_name)
                    .map(Value::String)
                    .unwrap_or(Value::Null)
            }
        }
    }
}

/// Decode a SQLite column value based on its type
fn decode_value_sqlite(
    row: &SqliteRow,
    col_name: &str,
    type_name: &str,
    mode: DateTimeMode,
) -> Value {
    let tn = type_name.to_ascii_uppercase();

    match tn.as_str() {
        // Boolean
        "BOOLEAN" | "BOOL" => decode_bool_with_fallback!(decode_sqlite_flat, row, col_name),

        // Integer
        "INTEGER" | "INT" => decode_sqlite_flat::<i64>(row, col_name)
            .map(Value::I64)
            .unwrap_or(Value::Null),

        // Real/Float
        "REAL" | "FLOAT" => decode_sqlite_flat::<f64>(row, col_name)
            .map(Value::F64)
            .unwrap_or(Value::Null),

        // Decimal
        "DECIMAL" | "NUMERIC" => decode_decimal_sqlite(row, col_name),

        // Text
        "TEXT" | "VARCHAR" | "CHAR" => decode_sqlite_flat::<String>(row, col_name)
            .map(Value::String)
            .unwrap_or(Value::Null),

        // Blob
        "BLOB" => decode_sqlite_flat::<Vec<u8>>(row, col_name)
            .map(Value::Bytes)
            .unwrap_or(Value::Null),

        // Date
        "DATE" => decode_sqlite_flat::<NaiveDate>(row, col_name)
            .map(naive_date_to_utc)
            .unwrap_or(Value::Null),

        // Time
        "TIME" => decode_sqlite_flat::<NaiveTime>(row, col_name)
            .map(|t| Value::String(t.to_string()))
            .unwrap_or(Value::Null),

        // Datetime/Timestamp
        "DATETIME" | "TIMESTAMP" => match mode {
            DateTimeMode::AsString => decode_sqlite_flat::<String>(row, col_name)
                .map(Value::String)
                .unwrap_or(Value::Null),
            DateTimeMode::AsUtc => {
                // Try NaiveDateTime first
                if let Some(ndt) = decode_sqlite_flat::<NaiveDateTime>(row, col_name) {
                    return Value::DateTime(ndt.and_utc());
                }
                // Fallback to string parsing
                decode_sqlite_flat::<String>(row, col_name)
                    .map(parse_datetime_from_string)
                    .unwrap_or(Value::Null)
            }
        },

        // Generic fallback
        _ => {
            // Try JSON first
            decode_sqlite_flat::<JsonValue>(row, col_name)
                .map(Value::Json)
                .or_else(|| decode_sqlite_flat::<String>(row, col_name).map(Value::String))
                .unwrap_or(Value::Null)
        }
    }
}

// =====================================================================
// PUBLIC API
// =====================================================================

/// Decodes a PostgreSQL row into a HashMap of column names to Values.
///
/// This function handles all common PostgreSQL types including:
/// - Integers (SMALLINT, INT, BIGINT)
/// - Floats (REAL, DOUBLE PRECISION)
/// - Decimals/Numeric types
/// - Text types (TEXT, VARCHAR, CHAR)
/// - Binary data (BYTEA)
/// - UUIDs
/// - JSON/JSONB
/// - Date/Time types (DATE, TIME, TIMESTAMP, TIMESTAMPTZ)
/// - Arrays (integer[], text[], etc.)
/// - Enums and custom types
///
/// # Arguments
/// * `row` - The PostgreSQL row to decode
///
/// # Returns
/// A HashMap where keys are column names and values are decoded `Value` enums
///
/// # Examples
/// ```no_run
/// use sqlx::postgres::PgRow;
/// let map = row_to_map_pg(&row);
/// ```
pub fn row_to_map_pg(row: &PgRow) -> Result<HashMap<String, Value>> {
    let mut map = HashMap::with_capacity(row.columns().len());

    for col in row.columns() {
        let name = col.name().to_string();
        let type_name = col.type_info().name();
        let value = decode_value_pg(row, &name, type_name);
        map.insert(name, value);
    }

    Ok(map)
}

/// Decodes a MySQL row into a HashMap of column names to Values.
///
/// Uses default DateTimeMode::AsUtc for datetime conversions.
///
/// # Arguments
/// * `row` - The MySQL row to decode
///
/// # Returns
/// A HashMap where keys are column names and values are decoded `Value` enums
pub fn row_to_map_mysql(row: &MySqlRow) -> Result<HashMap<String, Value>> {
    row_to_map_mysql_with_mode(row, DateTimeMode::default())
}

/// Decodes a MySQL row into a HashMap with custom DateTime handling.
///
/// # Arguments
/// * `row` - The MySQL row to decode
/// * `mode` - How to handle DATETIME/TIMESTAMP columns (AsUtc or AsString)
///
/// # Returns
/// A HashMap where keys are column names and values are decoded `Value` enums
pub fn row_to_map_mysql_with_mode(
    row: &MySqlRow,
    mode: DateTimeMode,
) -> Result<HashMap<String, Value>> {
    let mut map = HashMap::with_capacity(row.columns().len());

    for col in row.columns() {
        let name = col.name().to_string();
        let type_name = col.type_info().name();
        let value = decode_value_mysql(row, &name, type_name, mode);
        map.insert(name, value);
    }

    Ok(map)
}

/// Decodes a SQLite row into a HashMap of column names to Values.
///
/// Uses default DateTimeMode::AsUtc for datetime conversions.
///
/// # Arguments
/// * `row` - The SQLite row to decode
///
/// # Returns
/// A HashMap where keys are column names and values are decoded `Value` enums
pub fn row_to_map_sqlite(row: &SqliteRow) -> Result<HashMap<String, Value>> {
    row_to_map_sqlite_with_mode(row, DateTimeMode::default())
}

/// Decodes a SQLite row into a HashMap with custom DateTime handling.
///
/// # Arguments
/// * `row` - The SQLite row to decode
/// * `mode` - How to handle DATETIME/TIMESTAMP columns (AsUtc or AsString)
///
/// # Returns
/// A HashMap where keys are column names and values are decoded `Value` enums
pub fn row_to_map_sqlite_with_mode(
    row: &SqliteRow,
    mode: DateTimeMode,
) -> Result<HashMap<String, Value>> {
    let mut map = HashMap::with_capacity(row.columns().len());

    for col in row.columns() {
        let name = col.name().to_string();
        let type_name = col.type_info().name();
        let value = decode_value_sqlite(row, &name, type_name, mode);
        map.insert(name, value);
    }

    Ok(map)
}
