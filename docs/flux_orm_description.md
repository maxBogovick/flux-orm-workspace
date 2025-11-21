# FluxORM Framework Overview

## 1. Executive Summary
**FluxORM** is a modern, asynchronous Object-Relational Mapping (ORM) framework designed specifically for the Rust programming language. It acts as a high-level abstraction layer between Rust application logic and relational database management systems (RDBMS).

Its primary goal is to eliminate the "boilerplate" code associated with database interactions while maintaining the strict type safety and performance characteristics of Rust. FluxORM allows developers to interact with databases using Rust structs and methods rather than raw SQL strings, while automatically handling the complexities of different database dialects (PostgreSQL, MySQL, SQLite).

---

## 2. Core Purpose: Why was FluxORM created?

FluxORM was created to solve three specific challenges inherent in Rust database development:

### A. The "Impedance Mismatch"
Rust uses strict, static typing (Structs, Enums, Vectors), while SQL databases use loose or different typing systems (Tables, JSON, Arrays, Nulls).
* **Purpose:** FluxORM bridges this gap using a unified `Value` system. It automatically normalizes data types—converting complex types like `chrono::DateTime`, `Uuid`, and `serde_json::Value` into database-compatible formats during writes, and decoding them back into strong Rust types during reads.

### B. Dialect Fragmentation
Writing raw SQL often locks an application into a specific database vendor (e.g., using `$` placeholders for Postgres but `?` for MySQL).
* **Purpose:** FluxORM abstracts these differences. It uses a `Dialect` system to dynamically generate the correct SQL syntax, quoting rules, and placeholder symbols at runtime, allowing the same Rust code to run across SQLite, PostgreSQL, and MySQL without modification.

### C. Boilerplate Reduction
Standard database driver usage in Rust (`sqlx`) requires repetitive code to map rows to structs and write manual `INSERT/UPDATE` statements.
* **Purpose:** FluxORM utilizes advanced **Procedural Macros** (`derive(Model)`). By simply annotating a struct, the framework auto-generates the entire data access layer, including CRUD operations, field mapping, and relationship management.

---

## 3. What is FluxORM used for?

FluxORM is used to build data-intensive backend applications where maintainability, safety, and developer velocity are priorities. It is specifically used for:

### 1. Type-Safe Query Building
Instead of writing error-prone SQL strings, developers use a fluent builder pattern or a DSL (Domain Specific Language). FluxORM generates type-safe field accessors (e.g., `User::fields::EMAIL`) to ensure that queries are validated at compile-time.
* **Usage:** Constructing complex `WHERE` clauses with operators like `Equals`, `Between`, `In`, and `Like` without risking SQL injection.

### 2. Entity Relationship Management
FluxORM manages the associations between different data models. It replaces manual `JOIN` writing with semantic attributes.
* **Usage:** Defining relationships like `One-to-Many` (`#[has_many]`), `Many-to-One` (`#[belongs_to]`), and `Many-to-Many` (`#[belongs_to_many]`). The framework automatically generates the queries required to load related records or pivot tables.

### 3. Lifecycle Management & Automation
It automates repetitive database tasks that are often forgotten by developers.
* **Usage:**
    * **Timestamps:** Automatically updating `created_at` and `updated_at` fields.
    * **Soft Deletes:** logical deletion (setting a `deleted_at` timestamp) instead of physical row removal, allowing for data restoration.
    * **Hooks:** Running custom logic `before_create` or `after_update`.

### 4. Atomic Transactions
It ensures data integrity by grouping multiple operations into a single atomic unit.
* **Usage:** Executing a closure where all database changes are committed only if the entire block succeeds; otherwise, they are rolled back automatically.

---

## 4. Technical Architecture

FluxORM is built on several distinct architectural pillars:

### The Unified `Value` System
At the heart of the framework is the `Value` enum. This is an intermediate representation that can hold any database primitive (Integer, Float, String, Bool, Null) or complex type (UUID, DateTime, JSON, Bytes).
* **Role:** It decouples the query builder from the specific database driver. The builder constructs queries using `Value`s, and the `BindParam` module translates them to the specific driver inputs (e.g., serializing Arrays to JSON strings for SQLite/MySQL vs. native arrays for Postgres).

### The Macro Expansion Engine
The `flux_orm_macro` crate parses Rust structs at compile time.
* **Role:** It injects the implementation of the `Model` trait. It analyzes field attributes (like `#[flux(primary_key)]` or `#[flux(skip)]`) to generate optimized `to_values` and `from_values` mapping functions, preventing runtime reflection overhead.

### The Row Mapper
The `RowMapper` is the translation layer for reading data.
* **Role:** It handles the "messy" reality of databases. For example, it handles PostgreSQL's strict typing versus SQLite's loose typing, including fallback logic (e.g., treating an integer `1` as `true` for Booleans in MySQL).

---

## 5. Summary of Capabilities

| Feature | Description | Purpose |
| :--- | :--- | :--- |
| **Async CRUD** | Full `async/await` support for Create, Read, Update, Delete. | Non-blocking I/O for high-performance web servers. |
| **Strict Mode** | Optional validation layer via `#[derive(Validate)]`. | Ensures data integrity before it reaches the database. |
| **Migrations** | Code-first schema definition using `migration!`. | Version controls database schema changes alongside Rust code. |
| **Safe DSL** | `query!` macro for compile-time query validation. | Prevents runtime SQL errors by checking syntax during compilation. |

In conclusion, **FluxORM** is a comprehensive infrastructure tool created to make Rust developers productive immediately. It removes the need to write repetitive SQL code, ensures type safety across the database boundary, and provides a unified interface for the most popular open-source databases.