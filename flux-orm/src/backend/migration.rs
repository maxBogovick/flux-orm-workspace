// ============================================================================
// MIGRATION SYSTEM
// ============================================================================

use chrono::{DateTime, Utc};
use crate::backend::common_models::Value;
use crate::backend::errors::FluxError;
use crate::driver::dialect::Dialect;
use crate::Flux;

#[derive(Clone, Debug)]
pub struct Migration {
    pub version: i64,
    pub name: String,
    pub up: String,
    pub down: String,
}

impl Migration {
    pub fn new(version: i64, name: &str, up: &str, down: &str) -> Self {
        Self {
            version,
            name: name.to_string(),
            up: up.to_string(),
            down: down.to_string(),
        }
    }
}

pub struct MigrationRunner<'a> {
    db: &'a Flux,
}

impl<'a> MigrationRunner<'a> {
    pub fn new(db: &'a Flux) -> Self {
        Self { db }
    }

    pub async fn run(&self, migrations: &[Migration]) -> crate::backend::errors::Result<()> {
        self.ensure_migrations_table().await?;

        let applied = self.get_applied_migrations().await?;

        for migration in migrations {
            if !applied.contains(&migration.version) {
                self.apply_migration(migration).await?;
            }
        }

        Ok(())
    }

    pub async fn rollback(&self, migrations: &[Migration], steps: usize) -> crate::backend::errors::Result<()> {
        let mut applied = self.get_applied_migrations().await?;
        applied.sort_by(|a, b| b.cmp(a));

        for version in applied.iter().take(steps) {
            if let Some(migration) = migrations.iter().find(|m| m.version == *version) {
                self.rollback_migration(migration).await.map_err(|e| {
                    FluxError::Migration(format!(
                        "Failed to rollback migration {}: {}",
                        migration.name, e
                    ))
                })?;
            }
        }

        Ok(())
    }

    pub async fn status(&self) -> crate::backend::errors::Result<Vec<MigrationStatus>> {
        self.ensure_migrations_table().await?;

        let sql = "SELECT version, name, applied_at FROM flux_migrations ORDER BY version";
        let rows = self.db.backend.fetch_all(sql, &[]).await?;

        Ok(rows
            .into_iter()
            .map(|row| MigrationStatus {
                version: row.get("version").and_then(|v| v.as_i64()).unwrap_or(0),
                name: row
                    .get("name")
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
                applied_at: row.get("applied_at").and_then(|v| v.as_datetime()),
            })
            .collect())
    }

    async fn ensure_migrations_table(&self) -> crate::backend::errors::Result<()> {
        let sql = match self.db.backend.dialect() {
            Dialect::SQLite => {
                "CREATE TABLE IF NOT EXISTS flux_migrations (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    version INTEGER NOT NULL UNIQUE,
                    name TEXT NOT NULL,
                    applied_at DATETIME NOT NULL
                )"
            }
            Dialect::PostgreSQL => {
                "CREATE TABLE IF NOT EXISTS flux_migrations (
                    id SERIAL PRIMARY KEY,
                    version BIGINT NOT NULL UNIQUE,
                    name TEXT NOT NULL,
                    applied_at TIMESTAMP WITH TIME ZONE NOT NULL
                )"
            }
            Dialect::MySQL => {
                "CREATE TABLE IF NOT EXISTS flux_migrations (
                    id INT AUTO_INCREMENT PRIMARY KEY,
                    version BIGINT NOT NULL UNIQUE,
                    name VARCHAR(255) NOT NULL,
                    applied_at DATETIME NOT NULL
                )"
            }
            Dialect::MSSQL => {
                unimplemented!()
            }
        };

        self.db.backend.execute(sql, &[]).await?;
        Ok(())
    }

    async fn get_applied_migrations(&self) -> crate::backend::errors::Result<Vec<i64>> {
        let sql = "SELECT version FROM flux_migrations";
        let rows = self.db.backend.fetch_all(sql, &[]).await?;

        Ok(rows
            .iter()
            .filter_map(|row| row.get("version").and_then(|v| v.as_i64()))
            .collect())
    }

    async fn apply_migration(&self, migration: &Migration) -> crate::backend::errors::Result<()> {
        let mut tx = self.db.backend.begin_transaction().await?;

        tx.execute(&migration.up, &[]).await?;

        let sql = format!(
            "INSERT INTO flux_migrations (version, name, applied_at) VALUES ({}, {}, {})",
            tx.dialect().placeholder(1),
            tx.dialect().placeholder(2),
            tx.dialect().placeholder(3)
        );

        tx.execute(
            &sql,
            &[
                Value::I64(migration.version),
                Value::String(migration.name.clone()),
                Value::String(Utc::now().to_rfc3339()),
            ],
        )
            .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn rollback_migration(&self, migration: &Migration) -> crate::backend::errors::Result<()> {
        let mut tx = self.db.backend.begin_transaction().await?;

        tx.execute(&migration.down, &[]).await?;

        let sql = format!(
            "DELETE FROM flux_migrations WHERE version = {}",
            tx.dialect().placeholder(1)
        );

        tx.execute(&sql, &[Value::I64(migration.version)]).await?;

        tx.commit().await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MigrationStatus {
    pub version: i64,
    pub name: String,
    pub applied_at: Option<DateTime<Utc>>,
}