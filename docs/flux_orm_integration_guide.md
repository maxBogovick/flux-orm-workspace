# FluxORM v3.0 - Complete Integration Guide

## 📦 Project Setup

### 1. Create Workspace Structure

```bash
# Create workspace
mkdir flux-orm-workspace && cd flux-orm-workspace

# Create main ORM crate
cargo new --lib flux-orm

# Create derive macro crate
cargo new --lib flux-orm-derive

```

### 2. Configure Workspace Root

**File: `Cargo.toml`**

```toml
[workspace]
members = ["flux-orm", "flux-orm-derive"]
resolver = "2"
```

### 3. Configure flux-orm Crate

**File: `flux-orm/Cargo.toml`**

```toml
[package]
name = "flux-orm"
version = "3.0.0"
edition = "2021"

[dependencies]
tokio = { version = "1.35", features = ["full"] }
async-trait = "0.1"
sqlx = { version = "0.7", features = ["runtime-tokio-native-tls", "sqlite", "postgres", "mysql", "chrono", "uuid", "json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.6", features = ["v4", "serde"] }
thiserror = "1.0"
flux-orm-derive = { version = "3.0.0", path = "../flux-orm-derive", optional = true }

[features]
default = ["sqlite", "derive"]
derive = ["flux-orm-derive"]
```

**File: `flux-orm/src/lib.rs`**

Copy the complete ORM implementation from the first artifact.

### 4. Configure flux-orm-derive Crate

**File: `flux-orm-derive/Cargo.toml`**

```toml
[package]
name = "flux-orm-derive"
version = "3.0.0"
edition = "2021"

[lib]
proc-macro = true

[dependencies]
syn = { version = "2.0", features = ["full", "extra-traits"] }
quote = "1.0"
proc-macro2 = "1.0"
darling = "0.20"
```

**File: `flux-orm-derive/src/lib.rs`**

Copy the derive macro implementation from the derive macros artifact.

## 🚀 Quick Start Usage

### Basic Model Definition

```rust
use flux_orm::*;
use flux_orm_derive::*;
use chrono::{DateTime, Utc};

#[derive(Model, Debug, Clone)]
#[flux(table = "users", timestamps)]
pub struct User {
    #[flux(primary_key)]
    pub id: Option<i64>,
    
    #[flux(unique)]
    pub username: String,
    
    #[flux(unique)]
    pub email: String,
    
    pub bio: Option<String>,
    
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### CRUD Operations

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let db = Flux::sqlite("myapp.db").await?;
    
    // Create
    let user = db.insert(User {
        id: None,
        username: "alice".into(),
        email: "alice@example.com".into(),
        bio: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }).await?;
    
    // Read
    let user = db.find::<User>(1).await?.unwrap();
    
    // Update (timestamps auto-updated)
    db.update(user).await?;
    
    // Delete
    db.delete(user).await?;
    
    Ok(())
}
```

## 🔗 Relations Setup

### One-to-Many (HasMany)

```rust
#[derive(Model, Debug, Clone)]
#[flux(table = "posts")]
pub struct Post {
    #[flux(primary_key)]
    pub id: Option<i64>,
    pub title: String,
    pub author_id: i64,
}

impl User {
    pub async fn posts(&self, db: &Flux) -> Result<Vec<Post>> {
        self.load_many(db).await
    }
}

#[async_trait]
impl HasMany<Post> for User {
    fn foreign_key() -> &'static str {
        "author_id"
    }
}

// Usage
let user = db.find::<User>(1).await?.unwrap();
let posts = user.posts(&db).await?;
```

### One-to-One (HasOne)

```rust
#[derive(Model, Debug, Clone)]
#[flux(table = "profiles")]
pub struct Profile {
    #[flux(primary_key)]
    pub id: Option<i64>,
    pub user_id: i64,
    pub bio: Option<String>,
}

impl User {
    pub async fn profile(&self, db: &Flux) -> Result<Option<Profile>> {
        self.load_one(db).await
    }
}

#[async_trait]
impl HasOne<Profile> for User {
    fn foreign_key() -> &'static str {
        "user_id"
    }
}
```

### Belongs To

```rust
impl Post {
    pub async fn author(&self, db: &Flux) -> Result<Option<User>> {
        self.load_parent(db).await
    }
}

#[async_trait]
impl BelongsTo<User> for Post {
    fn foreign_key_value(&self) -> Option<i64> {
        Some(self.author_id)
    }
}
```

### Many-to-Many (BelongsToMany)

```rust
#[derive(Model, Debug, Clone)]
#[flux(table = "tags")]
pub struct Tag {
    #[flux(primary_key)]
    pub id: Option<i64>,
    pub name: String,
}

impl Post {
    pub async fn tags(&self, db: &Flux) -> Result<Vec<Tag>> {
        self.load_many(db).await
    }
}

#[async_trait]
impl BelongsToMany<Tag> for Post {
    fn pivot_table() -> &'static str {
        "post_tags"
    }
    
    fn foreign_key() -> &'static str {
        "post_id"
    }
    
    fn related_key() -> &'static str {
        "tag_id"
    }
}

// Create pivot table in migration
Migration::new(
    3,
    "create_post_tags",
    "CREATE TABLE post_tags (
        post_id INTEGER NOT NULL,
        tag_id INTEGER NOT NULL,
        PRIMARY KEY (post_id, tag_id),
        FOREIGN KEY (post_id) REFERENCES posts(id),
        FOREIGN KEY (tag_id) REFERENCES tags(id)
    )",
    "DROP TABLE post_tags"
)
```

## ⏰ Timestamps (Auto-managed)

```rust
#[derive(Model, Debug, Clone)]
#[flux(table = "posts", timestamps)]
pub struct Post {
    #[flux(primary_key)]
    pub id: Option<i64>,
    pub title: String,
    
    // Required fields for timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Timestamps are automatically set on create/update
let post = db.insert(post).await?; // created_at and updated_at set
db.update(post).await?;            // updated_at updated automatically
```

## 🗑️ Soft Deletes

```rust
#[derive(Model, Debug, Clone)]
#[flux(table = "posts", soft_delete)]
pub struct Post {
    #[flux(primary_key)]
    pub id: Option<i64>,
    pub title: String,
    
    // Required field for soft delete
    pub deleted_at: Option<DateTime<Utc>>,
}

// Soft delete
let mut post = db.find::<Post>(1).await?.unwrap();
post.soft_delete(&db).await?;  // Sets deleted_at

// Restore
post.restore(&db).await?;      // Clears deleted_at

// Force delete (permanent)
post.force_delete(&db).await?; // Actually deletes from DB
```

## 🔧 Migrations

```rust
async fn run_migrations(db: &Flux) -> Result<()> {
    let migrations = vec![
        Migration::new(
            1,
            "create_users",
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                email TEXT NOT NULL UNIQUE,
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL
            )",
            "DROP TABLE users"
        ),
        Migration::new(
            2,
            "create_posts",
            "CREATE TABLE posts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                author_id INTEGER NOT NULL,
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL,
                FOREIGN KEY (author_id) REFERENCES users(id)
            )",
            "DROP TABLE posts"
        ),
    ];
    
    db.migrate(&migrations).await
}
```

## 🔍 Query Builder

```rust
// Simple WHERE
let users = db.query(
    Query::<User>::new()
        .where_eq("active", true)
).await?;

// Multiple conditions
let posts = db.query(
    Query::<Post>::new()
        .where_eq("published", true)
        .where_gt("views", 100)
        .where_like("title", "%rust%")
).await?;

// IN query
let users = db.query(
    Query::<User>::new()
        .where_in("id", vec![1, 2, 3])
).await?;

// Order and pagination
let posts = db.query(
    Query::<Post>::new()
        .order_by_desc("created_at")
        .limit(10)
        .offset(20)
).await?;

// Pagination helper
let page = db.paginate(
    Query::<Post>::new(),
    1,  // page number
    20  // per page
).await?;

println!("Page {}/{}", page.page, page.total_pages);
```

## 💳 Transactions

```rust
db.transaction(|tx| Box::pin(async move {
    // Create user
    // Create profile
    // All succeed or all rollback
    Ok(())
})).await?;

// With error handling
let result = db.transaction(|tx| Box::pin(async move {
    // Operations here
    if something_wrong {
        return Err(FluxError::Transaction("Failed".into()));
    }
    Ok(result)
})).await;

match result {
    Ok(data) => println!("Success: {:?}", data),
    Err(e) => println!("Rolled back: {:?}", e),
}
```

## 🔌 Multiple Databases

```rust
// SQLite
let db = Flux::sqlite("sqlite:myapp.db").await?;
let db = Flux::sqlite("sqlite::memory:").await?; // In-memory

// PostgreSQL
let db = Flux::postgres("postgresql://user:pass@localhost/myapp").await?;

// MySQL
let db = Flux::mysql("mysql://user:pass@localhost/myapp").await?;
```

## 📊 Aggregates

```rust
// Count
let total = db.count(Query::<User>::new()).await?;

let active = db.count(
    Query::<User>::new().where_eq("active", true)
).await?;

// Exists
let exists = db.exists(
    Query::<User>::new().where_eq("email", "test@example.com")
).await?;

// First
let user = db.first(
    Query::<User>::new()
        .order_by("created_at")
).await?;
```

## 🧪 Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_user_creation() -> Result<()> {
        let db = Flux::sqlite("sqlite::memory:").await?;
        
        // Run migrations
        setup_test_db(&db).await?;
        
        // Test
        let user = db.insert(User {
            id: None,
            username: "test".into(),
            email: "test@example.com".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }).await?;
        
        assert!(user.id.is_some());
        assert_eq!(user.username, "test");
        
        Ok(())
    }
}
```

## 🚀 Production Deployment

### 1. Build for Production

```bash
cargo build --release --workspace
```

### 2. Environment Configuration

```rust
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    
    let db = Flux::sqlite(&database_url).await?;
    
    // Run migrations
    db.migrate(&get_migrations()).await?;
    
    Ok(())
}
```

### 3. Connection Pool Configuration

```rust
let db = Flux::sqlite(&database_url)
    .await?
    .with_config(FluxConfig {
        query_logging: cfg!(debug_assertions),
        auto_timestamps: true,
    });
```

## 📝 Best Practices

### 1. Always Use Transactions for Multi-Step Operations

```rust
db.transaction(|tx| Box::pin(async move {
    let user = /* create user */;
    let profile = /* create profile */;
    Ok(())
})).await?;
```

### 2. Use Relations for Related Data

```rust
// Good: Use relations
let posts = user.posts(&db).await?;

// Avoid: Manual queries
let posts = db.query(
    Query::<Post>::new().where_eq("author_id", user.id.unwrap())
).await?;
```

### 3. Enable Timestamps for Audit Trails

```rust
#[derive(Model, Debug, Clone)]
#[flux(table = "important_data", timestamps)]
pub struct ImportantData {
    // ... fields
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### 4. Use Soft Deletes for Critical Data

```rust
#[derive(Model, Debug, Clone)]
#[flux(table = "users", soft_delete)]
pub struct User {
    // ... fields
    pub deleted_at: Option<DateTime<Utc>>,
}
```

## 🐛 Common Issues

### Issue: Model trait not implemented

**Solution**: Make sure you have `#[derive(Model)]` and all required fields:

```rust
#[derive(Model, Debug, Clone)]  // Add Model derive
#[flux(table = "users")]
pub struct User {
    #[flux(primary_key)]  // Mark primary key
    pub id: Option<i64>,   // Must be Option<T>
    // ...
}
```

### Issue: Timestamps not working

**Solution**: Include `timestamps` in `#[flux]` and add required fields:

```rust
#[derive(Model, Debug, Clone)]
#[flux(table = "users", timestamps)]  // Enable timestamps
pub struct User {
    pub id: Option<i64>,
    // Required fields:
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### Issue: Relations not loading

**Solution**: Implement the relation trait:

```rust
#[async_trait]
impl HasMany<Post> for User {
    fn foreign_key() -> &'static str {
        "author_id"  // Foreign key in posts table
    }
}
```

## 🎯 Summary

FluxORM v3.0 with derive macros provides:

1. **Zero Boilerplate** - Models defined with simple attributes
2. **Type Safety** - Compile-time guarantees
3. **Production Ready** - Real SQLx integration
4. **Full Features** - Relations, timestamps, soft deletes, transactions
5. **Clean API** - Intuitive and ergonomic

You now have a complete, production-ready ORM framework!