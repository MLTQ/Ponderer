use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use uuid::Uuid;

use super::{MemoryBackend, MemoryDesignVersion, WorkingMemoryEntry};

/// Candidate backend using FTS5 index alongside a canonical docs table.
pub struct FtsMemoryBackendV2;

impl FtsMemoryBackendV2 {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FtsMemoryBackendV2 {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBackend for FtsMemoryBackendV2 {
    fn design_version(&self) -> MemoryDesignVersion {
        MemoryDesignVersion {
            design_id: "fts_v2".to_string(),
            schema_version: 2,
        }
    }

    fn set_entry(&self, conn: &Connection, key: &str, content: &str) -> Result<()> {
        ensure_fts_tables(conn)?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO working_memory_fts_docs (key, content, updated_at)
             VALUES (?1, ?2, ?3)",
            params![key, content, now],
        )?;

        // Keep the FTS index in sync for future search-oriented retrieval.
        conn.execute("DELETE FROM working_memory_fts_index WHERE key = ?1", [key])?;
        conn.execute(
            "INSERT INTO working_memory_fts_index (key, content) VALUES (?1, ?2)",
            params![key, content],
        )?;
        Ok(())
    }

    fn get_entry(&self, conn: &Connection, key: &str) -> Result<Option<WorkingMemoryEntry>> {
        ensure_fts_tables(conn)?;
        let result = conn.query_row(
            "SELECT key, content, updated_at
             FROM working_memory_fts_docs
             WHERE key = ?1",
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
        ensure_fts_tables(conn)?;
        let mut stmt = conn.prepare(
            "SELECT key, content, updated_at
             FROM working_memory_fts_docs
             ORDER BY updated_at DESC",
        )?;

        let rows = stmt
            .query_map([], |row| {
                Ok(WorkingMemoryEntry {
                    key: row.get(0)?,
                    content: row.get(1)?,
                    updated_at: parse_rfc3339(row.get::<_, String>(2)?, 2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    fn delete_entry(&self, conn: &Connection, key: &str) -> Result<()> {
        ensure_fts_tables(conn)?;
        conn.execute("DELETE FROM working_memory_fts_docs WHERE key = ?1", [key])?;
        conn.execute("DELETE FROM working_memory_fts_index WHERE key = ?1", [key])?;
        Ok(())
    }
}

/// Candidate backend that models memory as append-only episodes with active pointers.
pub struct EpisodicMemoryBackendV3;

impl EpisodicMemoryBackendV3 {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EpisodicMemoryBackendV3 {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBackend for EpisodicMemoryBackendV3 {
    fn design_version(&self) -> MemoryDesignVersion {
        MemoryDesignVersion {
            design_id: "episodic_v3".to_string(),
            schema_version: 3,
        }
    }

    fn set_entry(&self, conn: &Connection, key: &str, content: &str) -> Result<()> {
        ensure_episodic_tables(conn)?;
        conn.execute(
            "UPDATE working_memory_episodes
             SET active = 0
             WHERE memory_key = ?1 AND active = 1",
            [key],
        )?;

        conn.execute(
            "INSERT INTO working_memory_episodes
             (episode_id, memory_key, content, updated_at, active)
             VALUES (?1, ?2, ?3, ?4, 1)",
            params![
                Uuid::new_v4().to_string(),
                key,
                content,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    fn get_entry(&self, conn: &Connection, key: &str) -> Result<Option<WorkingMemoryEntry>> {
        ensure_episodic_tables(conn)?;
        let result = conn.query_row(
            "SELECT memory_key, content, updated_at
             FROM working_memory_episodes
             WHERE memory_key = ?1 AND active = 1
             ORDER BY updated_at DESC
             LIMIT 1",
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
        ensure_episodic_tables(conn)?;
        let mut stmt = conn.prepare(
            "SELECT memory_key, content, updated_at
             FROM working_memory_episodes
             WHERE active = 1
             ORDER BY updated_at DESC",
        )?;

        let rows = stmt
            .query_map([], |row| {
                Ok(WorkingMemoryEntry {
                    key: row.get(0)?,
                    content: row.get(1)?,
                    updated_at: parse_rfc3339(row.get::<_, String>(2)?, 2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    fn delete_entry(&self, conn: &Connection, key: &str) -> Result<()> {
        ensure_episodic_tables(conn)?;
        conn.execute(
            "UPDATE working_memory_episodes
             SET active = 0
             WHERE memory_key = ?1",
            [key],
        )?;
        Ok(())
    }
}

fn ensure_fts_tables(conn: &Connection) -> Result<()> {
    conn.execute(
        r#"CREATE TABLE IF NOT EXISTS working_memory_fts_docs (
            key TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )"#,
        [],
    )?;

    conn.execute(
        r#"CREATE VIRTUAL TABLE IF NOT EXISTS working_memory_fts_index
            USING fts5(key, content)"#,
        [],
    )?;
    Ok(())
}

fn ensure_episodic_tables(conn: &Connection) -> Result<()> {
    conn.execute(
        r#"CREATE TABLE IF NOT EXISTS working_memory_episodes (
            episode_id TEXT PRIMARY KEY,
            memory_key TEXT NOT NULL,
            content TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            active INTEGER NOT NULL DEFAULT 1
        )"#,
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_working_memory_episodes_key_active
         ON working_memory_episodes(memory_key, active)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_working_memory_episodes_updated_at
         ON working_memory_episodes(updated_at DESC)",
        [],
    )?;
    Ok(())
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
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn fts_backend_roundtrip_core_api() {
        let conn = setup_conn();
        let backend = FtsMemoryBackendV2::new();

        backend
            .set_entry(&conn, "release", "ship fts backend")
            .unwrap();
        let got = backend.get_entry(&conn, "release").unwrap().unwrap();
        assert_eq!(got.content, "ship fts backend");

        let all = backend.list_entries(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].key, "release");

        backend.delete_entry(&conn, "release").unwrap();
        assert!(backend.get_entry(&conn, "release").unwrap().is_none());
    }

    #[test]
    fn episodic_backend_roundtrip_core_api() {
        let conn = setup_conn();
        let backend = EpisodicMemoryBackendV3::new();

        backend.set_entry(&conn, "focus", "alpha").unwrap();
        backend.set_entry(&conn, "focus", "beta").unwrap();
        let got = backend.get_entry(&conn, "focus").unwrap().unwrap();
        assert_eq!(got.content, "beta");

        let all = backend.list_entries(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].key, "focus");

        backend.delete_entry(&conn, "focus").unwrap();
        assert!(backend.get_entry(&conn, "focus").unwrap().is_none());
    }

    #[test]
    fn episodic_backend_preserves_history_rows() {
        let conn = setup_conn();
        let backend = EpisodicMemoryBackendV3::new();

        backend.set_entry(&conn, "topic", "v1").unwrap();
        backend.set_entry(&conn, "topic", "v2").unwrap();
        backend.set_entry(&conn, "topic", "v3").unwrap();

        let total_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM working_memory_episodes", [], |row| {
                row.get(0)
            })
            .unwrap();
        let active_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM working_memory_episodes WHERE active = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(total_rows, 3);
        assert_eq!(active_rows, 1);
    }
}
