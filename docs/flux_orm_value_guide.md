# 📘 Flux ORM — `Value` Type: Complete Technical Documentation

## Overview

`Value` is a central, strongly-typed data container used across the Flux ORM stack.
It represents all possible database values in a type-safe, serializable, and extensible manner.

The purpose of `Value` is to:

* unify all supported database value types under a single enum;
* provide safe conversion mechanisms (both strict and coercive);
* allow high-level abstractions (models, query builders, row mappers) to interact with DB data without generics explosion;
* ensure ergonomic and predictable handling of database nullability and dynamic typing.

`Value` is designed similarly to PostgreSQL's internal value model and JSON concepts but keeps **strict typing**, **predictable coercion rules**, and **explicit error handling**.

---

# 1. Enum Definition

```rust
/// A strongly-typed dynamic value container used throughout Flux ORM.
///
/// `Value` can represent all commonly used SQL data types, as well as
/// extended ORM-level abstractions such as JSON, UUID, DateTime, and enums.
///
/// Each variant is serializable, deserializable, convertible, and safe to use
/// in database queries, row mapping, and query builders.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Null,
    Bool(bool),

    I16(i16),
    I32(i32),
    I64(i64),

    F32(f32),
    F64(f64),

    String(String),
    Bytes(Vec<u8>),

    DateTime(DateTime<Utc>),
    Uuid(Uuid),
    Json(serde_json::Value),

    /// A homogeneous list of `Value`s.
    Array(Vec<Value>),

    /// Strict enum type used for database enums and constrained domain fields.
    Enum(String),
}
```

---

# 2. Design Goals

### 2.1 Type Safety

All variants carry explicit types. No “implicit JSON” or “untyped text” conversions happen without developer consent.

### 2.2 Zero-Cost Conversions

All `From<T>` implementations avoid unnecessary allocations.

### 2.3 Extensibility

New variants can be introduced without breaking existing code
(e.g., support for Decimal, BigInt, IP address types, Geospatial, etc.).

### 2.4 Predictable Coercion Rules

Conversions follow strict guidelines:

* **Exact match preferred**
* **Lossless numeric coercion allowed**
* **Lossy coercion only via `TryFrom<Value>`**
* **No automatic null-to-zero or null-to-empty conversions**

---

# 3. Conversions (From / TryFrom)

## 3.1 Implicit Conversions via `From<T>`

`Value` supports ergonomic creation from native Rust types:

```rust
let v1: Value = 100_i32.into();
let v2: Value = "Hello".into();
let v3: Value = Some(55_i64).into();
let v4: Value = None::<i32>.into(); // -> Value::Null
let v5: Value = uuid::Uuid::new_v4().into();
```

### Supported `From<T>` Implementations

| Rust Type              | Result                            |
| ---------------------- | --------------------------------- |
| `bool`                 | `Value::Bool`                     |
| integers (i16/i32/i64) | `Value::I*`                       |
| floats (f32/f64)       | `Value::F*`                       |
| `String`, `&str`       | `Value::String`                   |
| `Vec<u8>`              | `Value::Bytes`                    |
| `Uuid`                 | `Value::Uuid`                     |
| `DateTime<Utc>`        | `Value::DateTime`                 |
| `serde_json::Value`    | `Value::Json`                     |
| `Option<T>`            | `Value::Null` or `Value::from(T)` |

---

## 3.2 Strict Conversions via `TryFrom<Value>`

`TryFrom` is used for retrieving concrete Rust types from a `Value`.

### Rules:

* Numeric coercion is **lossy** (uses `as`), but predictable.
* Null always results in an error.
* String parsing is allowed for complex types (UUID, DateTime, JSON).
* Error messages use unified `FluxError::Serialization`.

### Examples

```rust
let v = Value::I64(500);

let x: i32 = v.clone().try_into()?;  // OK, lossy narrowing allowed
let y: i64 = v.try_into()?;          // Exact match
```

```rust
let uuid_val = Value::String("550e8400-e29b-41d4-a716-446655440000".into());
let uuid: Uuid = uuid_val.try_into()?; // parsed from string
```

### Example: Handling Errors

```rust
match val.try_into() as Result<i32> {
    Ok(n) => println!("value = {}", n),
    Err(e) => println!("error: {}", e),
}
```

---

# 4. Accessor Methods (as_*)

Accessor methods provide ergonomic and safe ways to work with values.

## 4.1 Null Check

```rust
pub fn is_null(&self) -> bool
```

## 4.2 Reference Accessors (Zero-Copy)

```rust
as_str(&self) -> Option<&str>
as_bytes(&self) -> Option<&[u8]>
as_datetime(&self) -> Option<DateTime<Utc>>
as_uuid(&self) -> Option<Uuid>
```

These do **not** allocate.

---

## 4.3 Owned Accessors (Clone)

```rust
as_string(&self) -> Option<String>
as_json(&self) -> Option<serde_json::Value>
```

These return owned copies.

---

## 4.4 Safe Coercion Accessors

These methods return `Option<T>` instead of `Result<T>`, and implement **boundary-checked numeric conversions**.

| Method      | Description                                        |
| ----------- | -------------------------------------------------- |
| `as_i16()`  | Returns `Some(i16)` if value fits within i16 range |
| `as_i32()`  | Range-checked extraction                           |
| `as_i64()`  | Lossless upcast                                    |
| `as_f32()`  | Converts f64 → f32                                 |
| `as_f64()`  | Lossless conversion                                |
| `as_bool()` | Integers: 0 → false, else true                     |

### Example

```rust
let v = Value::I32(999);
assert_eq!(v.as_i16(), None);   // out of range
assert_eq!(v.as_i32(), Some(999));
assert_eq!(v.as_i64(), Some(999));
```

---

# 5. Error Handling

All conversion failures produce consistent, developer-friendly errors.

```rust
FluxError::Serialization(format!(
    "Cannot convert {:?} to {}",
    value, target
))
```

### Example Error

```
SerializationError: Cannot convert Null to i32
```

This ensures predictable behavior across the ORM.

---

# 6. Recommended Usage Patterns

## 6.1 Row Mapping

Every row returned from a database driver can be mapped to `HashMap<String, Value>`.

`Value` then acts as a typed intermediate layer between:

* low-level database raw types
* high-level model types
* ORM’s schema/derive system

---

## 6.2 Query Builder Parameters

The query builder uses `Value` for:

* WHERE parameters
* INSERT/UPDATE sets
* IN (…) array values
* Dynamic expressions

Example:

```rust
builder.where_eq("age", 30.into());
builder.where_ne("status", Value::Enum("banned".into()));
```

---

# 7. Future Extensions

`Value` is designed to support extension for:

### Planned additions:

* `Decimal`
* `BigInt`
* `IpAddr`
* `GeoPoint`
* `Duration`
* `Date` / `Time` split support

The enum layout and trait implementations allow these to be added without breaking the API.

---

# 8. Summary

`Value` is a foundational component of Flux ORM that provides:

* strictly typed storage of DB values
* ergonomic conversions
* predictable error handling
* safe coercions
* zero-copy getters
* unified interface for all ORM layers

It is designed to be stable, extensible, and high-performance in production workloads.

---
