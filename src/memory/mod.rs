pub mod archive;
pub mod candidate_backends;
pub mod eval;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

pub const MEMORY_DESIGN_STATE_KEY: &str = "memory_design_id";
pub const MEMORY_SCHEMA_VERSION_STATE_KEY: &str = "memory_schema_version";
pub const DEFAULT_MEMORY_DESIGN_ID: &str = "kv_v1";
pub const DEFAULT_MEMORY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDesignVersion {
    pub design_id: String,
    pub schema_version: u32,
}

impl MemoryDesignVersion {
    pub fn kv_v1() -> Self {
        Self {
            design_id: DEFAULT_MEMORY_DESIGN_ID.to_string(),
            schema_version: DEFAULT_MEMORY_SCHEMA_VERSION,
        }
    }
}

impl Default for MemoryDesignVersion {
    fn default() -> Self {
        Self::kv_v1()
    }
}

/// A working memory entry - persistent notes the agent can reference and update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingMemoryEntry {
    pub key: String,
    pub content: String,
    pub updated_at: DateTime<Utc>,
}

/// Versioned backend interface for memory operations.
///
/// This allows Ponderer to keep memory API stable while swapping storage/indexing
/// designs (e.g., KV, FTS, episodic) behind the same contract.
pub trait MemoryBackend: Send + Sync {
    fn design_version(&self) -> MemoryDesignVersion;
    fn set_entry(&self, conn: &Connection, key: &str, content: &str) -> Result<()>;
    fn get_entry(&self, conn: &Connection, key: &str) -> Result<Option<WorkingMemoryEntry>>;
    fn list_entries(&self, conn: &Connection) -> Result<Vec<WorkingMemoryEntry>>;
    fn delete_entry(&self, conn: &Connection, key: &str) -> Result<()>;
}

/// Baseline KV memory backend that preserves current behavior.
pub struct KvMemoryBackend;

impl KvMemoryBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for KvMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBackend for KvMemoryBackend {
    fn design_version(&self) -> MemoryDesignVersion {
        MemoryDesignVersion::kv_v1()
    }

    fn set_entry(&self, conn: &Connection, key: &str, content: &str) -> Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO working_memory (key, content, updated_at) VALUES (?1, ?2, ?3)",
            params![key, content, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    fn get_entry(&self, conn: &Connection, key: &str) -> Result<Option<WorkingMemoryEntry>> {
        let result = conn.query_row(
            "SELECT key, content, updated_at FROM working_memory WHERE key = ?1",
            [key],
            |row| {
                Ok(WorkingMemoryEntry {
                    key: row.get(0)?,
                    content: row.get(1)?,
                    updated_at: parse_rfc3339(row.get::<_, String>(2)?, 2)?,
                })
            },
        );

        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn list_entries(&self, conn: &Connection) -> Result<Vec<WorkingMemoryEntry>> {
        let mut stmt = conn.prepare(
            "SELECT key, content, updated_at FROM working_memory ORDER BY updated_at DESC",
        )?;

        let entries = stmt
            .query_map([], |row| {
                Ok(WorkingMemoryEntry {
                    key: row.get(0)?,
                    content: row.get(1)?,
                    updated_at: parse_rfc3339(row.get::<_, String>(2)?, 2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    fn delete_entry(&self, conn: &Connection, key: &str) -> Result<()> {
        conn.execute("DELETE FROM working_memory WHERE key = ?1", [key])?;
        Ok(())
    }
}

pub struct MemoryMigration {
    pub id: &'static str,
    pub from: MemoryDesignVersion,
    pub to: MemoryDesignVersion,
    pub apply: fn(&Connection) -> Result<()>,
}

/// Migration registry scaffolding for future memory design upgrades.
///
/// For now we only support direct migrations. Multi-hop path planning can be
/// added later when multiple migration edges exist.
#[derive(Default)]
pub struct MemoryMigrationRegistry {
    migrations: Vec<MemoryMigration>,
}

impl MemoryMigrationRegistry {
    pub fn register(&mut self, migration: MemoryMigration) {
        self.migrations.push(migration);
    }

    pub fn find_direct(
        &self,
        from: &MemoryDesignVersion,
        to: &MemoryDesignVersion,
    ) -> Option<&MemoryMigration> {
        self.migrations
            .iter()
            .find(|m| m.from == *from && m.to == *to)
    }

    pub fn apply_direct(
        &self,
        conn: &Connection,
        from: &MemoryDesignVersion,
        to: &MemoryDesignVersion,
    ) -> Result<()> {
        let migration = self.find_direct(from, to).with_context(|| {
            format!(
                "No memory migration registered from {}:{} to {}:{}",
                from.design_id, from.schema_version, to.design_id, to.schema_version
            )
        })?;
        (migration.apply)(conn)
            .with_context(|| format!("Failed to apply memory migration '{}'", migration.id))
    }
}

fn parse_rfc3339(
    value: String,
    column: usize,
) -> std::result::Result<DateTime<Utc>, rusqlite::Error> {
    value.parse().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            r#"CREATE TABLE working_memory (
                key TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )"#,
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn kv_backend_roundtrip() {
        let conn = setup_conn();
        let backend = KvMemoryBackend::new();

        backend
            .set_entry(&conn, "focus", "ship memory backend")
            .unwrap();
        let entry = backend.get_entry(&conn, "focus").unwrap().unwrap();
        assert_eq!(entry.key, "focus");
        assert_eq!(entry.content, "ship memory backend");

        let all = backend.list_entries(&conn).unwrap();
        assert_eq!(all.len(), 1);

        backend.delete_entry(&conn, "focus").unwrap();
        assert!(backend.get_entry(&conn, "focus").unwrap().is_none());
    }

    #[test]
    fn migration_registry_reports_missing_path() {
        let conn = setup_conn();
        let registry = MemoryMigrationRegistry::default();
        let err = registry
            .apply_direct(
                &conn,
                &MemoryDesignVersion::kv_v1(),
                &MemoryDesignVersion {
                    design_id: "fts_v2".to_string(),
                    schema_version: 2,
                },
            )
            .unwrap_err();
        assert!(err.to_string().contains("No memory migration registered"));
    }

    #[test]
    fn migration_registry_applies_direct_migration() {
        let conn = setup_conn();
        conn.execute(
            r#"CREATE TABLE migration_probe (
                id INTEGER PRIMARY KEY,
                applied INTEGER NOT NULL
            )"#,
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO migration_probe (id, applied) VALUES (1, 0)",
            [],
        )
        .unwrap();

        let mut registry = MemoryMigrationRegistry::default();
        registry.register(MemoryMigration {
            id: "kv_v1_to_kv_v2_probe",
            from: MemoryDesignVersion::kv_v1(),
            to: MemoryDesignVersion {
                design_id: "kv_v2".to_string(),
                schema_version: 2,
            },
            apply: |conn| {
                conn.execute("UPDATE migration_probe SET applied = 1 WHERE id = 1", [])?;
                Ok(())
            },
        });

        registry
            .apply_direct(
                &conn,
                &MemoryDesignVersion::kv_v1(),
                &MemoryDesignVersion {
                    design_id: "kv_v2".to_string(),
                    schema_version: 2,
                },
            )
            .unwrap();

        let applied: i64 = conn
            .query_row(
                "SELECT applied FROM migration_probe WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(applied, 1);
    }
}
