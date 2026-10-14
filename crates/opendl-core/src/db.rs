use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Queued => "queued",
            TaskStatus::Downloading => "downloading",
            TaskStatus::Paused => "paused",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "downloading" => TaskStatus::Downloading,
            "paused" => TaskStatus::Paused,
            "completed" => TaskStatus::Completed,
            "failed" => TaskStatus::Failed,
            _ => TaskStatus::Queued,
        }
    }
}

fn default_queue_id() -> String {
    "main".to_string()
}

fn default_on_complete() -> Option<String> {
    Some("none".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueRecord {
    pub id: String,
    pub name: String,
    pub max_concurrent: usize,
    pub speed_limit_kbps: u64,
    pub is_active: bool,
    pub schedule_start: Option<String>,
    pub schedule_stop: Option<String>,
    pub created_at: u64,
    #[serde(default = "default_on_complete")]
    pub on_complete_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: Uuid,
    pub url: String,
    pub filename: String,
    pub destination: PathBuf,
    pub total_size: Option<u64>,
    pub downloaded_bytes: u64,
    pub status: TaskStatus,
    pub category: String,
    pub etag: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default = "default_queue_id")]
    pub queue_id: String,
    #[serde(default)]
    pub queue_order: i64,
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open SQLite database at {:?}", path))?;

        // Enable WAL mode and optimize pragmas
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        info!(path = ?path, "SQLite database opened with WAL mode");
        Ok(db)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                filename TEXT NOT NULL,
                destination TEXT NOT NULL,
                total_size INTEGER,
                downloaded_bytes INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                category TEXT NOT NULL,
                etag TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                queue_id TEXT NOT NULL DEFAULT 'main',
                queue_order INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS categories (
                name TEXT PRIMARY KEY,
                extensions TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS queues (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                max_concurrent INTEGER NOT NULL DEFAULT 2,
                speed_limit_kbps INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 1,
                schedule_start TEXT,
                schedule_stop TEXT,
                created_at INTEGER NOT NULL,
                on_complete_action TEXT DEFAULT 'none'
            );",
        )?;

        // Migration check for tasks table existing from earlier version
        let has_queue_id: bool = conn
            .prepare("SELECT queue_id FROM tasks LIMIT 1")
            .is_ok();
        if !has_queue_id {
            let _ = conn.execute("ALTER TABLE tasks ADD COLUMN queue_id TEXT NOT NULL DEFAULT 'main'", []);
        }
        let has_queue_order: bool = conn
            .prepare("SELECT queue_order FROM tasks LIMIT 1")
            .is_ok();
        if !has_queue_order {
            let _ = conn.execute("ALTER TABLE tasks ADD COLUMN queue_order INTEGER NOT NULL DEFAULT 0", []);
        }

        let has_on_complete: bool = conn
            .prepare("SELECT on_complete_action FROM queues LIMIT 1")
            .is_ok();
        if !has_on_complete {
            let _ = conn.execute("ALTER TABLE queues ADD COLUMN on_complete_action TEXT DEFAULT 'none'", []);
        }

        // Seed default queues: Main Queue and Night Queue
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        conn.execute(
            "INSERT OR IGNORE INTO queues (id, name, max_concurrent, speed_limit_kbps, is_active, schedule_start, schedule_stop, created_at, on_complete_action)
             VALUES ('main', 'Main Queue', 2, 0, 1, NULL, NULL, ?1, 'none')",
            params![now as i64],
        )?;

        conn.execute(
            "INSERT OR IGNORE INTO queues (id, name, max_concurrent, speed_limit_kbps, is_active, schedule_start, schedule_stop, created_at, on_complete_action)
             VALUES ('night', 'Night Queue', 1, 0, 0, '01:00', '07:00', ?1, 'none')",
            params![now as i64],
        )?;

        // Seed default categories
        conn.execute(
            "INSERT OR IGNORE INTO categories (name, extensions) VALUES
                ('Compressed', 'zip,rar,7z,tar,gz,bz2,iso,xz'),
                ('Documents', 'doc,docx,pdf,txt,ppt,pptx,xls,xlsx,epub'),
                ('Music', 'mp3,wav,flac,aac,m4a,ogg,wma'),
                ('Programs', 'exe,msi,bat,cmd,apk,dmg,appx'),
                ('Video', 'mp4,mkv,avi,mov,flv,webm,ts,m4v,wmv');",
            [],
        )?;

        Ok(())
    }

    pub fn classify_category(&self, filename: &str) -> String {
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ext.is_empty() {
            return "General".to_string();
        }

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT name, extensions FROM categories")
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                ))
            })
            .unwrap();

        for row in rows.flatten() {
            let (cat_name, exts) = row;
            for e in exts.split(',') {
                if e.trim().eq_ignore_ascii_case(&ext) {
                    return cat_name;
                }
            }
        }

        "General".to_string()
    }

    pub fn insert_task(&self, task: &TaskRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO tasks (
                id, url, filename, destination, total_size, downloaded_bytes,
                status, category, etag, created_at, updated_at, queue_id, queue_order
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                task.id.to_string(),
                task.url,
                task.filename,
                task.destination.to_string_lossy(),
                task.total_size.map(|s| s as i64),
                task.downloaded_bytes as i64,
                task.status.as_str(),
                task.category,
                task.etag,
                task.created_at as i64,
                task.updated_at as i64,
                task.queue_id,
                task.queue_order,
            ],
        )?;
        debug!(id = %task.id, "Task inserted into database");
        Ok(())
    }

    pub fn update_task_metadata(
        &self,
        id: Uuid,
        filename: &str,
        destination: &Path,
        total_size: Option<u64>,
        category: &str,
        etag: Option<&str>,
    ) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET filename = ?1, destination = ?2, total_size = ?3, category = ?4, etag = ?5, updated_at = ?6 WHERE id = ?7",
            params![
                filename,
                destination.to_string_lossy(),
                total_size.map(|s| s as i64),
                category,
                etag,
                now as i64,
                id.to_string()
            ],
        )?;
        Ok(())
    }

    pub fn update_task_progress(&self, id: Uuid, downloaded: u64, status: TaskStatus) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET downloaded_bytes = ?1, status = ?2, updated_at = ?3 WHERE id = ?4",
            params![downloaded as i64, status.as_str(), now as i64, id.to_string()],
        )?;
        Ok(())
    }

    pub fn get_task(&self, id: Uuid) -> Result<Option<TaskRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, filename, destination, total_size, downloaded_bytes,
                    status, category, etag, created_at, updated_at, queue_id, queue_order
             FROM tasks WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![id.to_string()])?;
        if let Some(row) = rows.next()? {
            let id_str: String = row.get(0)?;
            let total_size: Option<i64> = row.get(4)?;
            let downloaded: i64 = row.get(5)?;
            let status_str: String = row.get(6)?;
            let created_at: i64 = row.get(9)?;
            let updated_at: i64 = row.get(10)?;
            let queue_id: String = row.get(11).unwrap_or_else(|_| "main".to_string());
            let queue_order: i64 = row.get(12).unwrap_or(0);

            Ok(Some(TaskRecord {
                id: Uuid::parse_str(&id_str)?,
                url: row.get(1)?,
                filename: row.get(2)?,
                destination: PathBuf::from(row.get::<_, String>(3)?),
                total_size: total_size.map(|s| s as u64),
                downloaded_bytes: downloaded as u64,
                status: TaskStatus::from_str(&status_str),
                category: row.get(7)?,
                etag: row.get(8)?,
                created_at: created_at as u64,
                updated_at: updated_at as u64,
                queue_id,
                queue_order,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_all_tasks(&self) -> Result<Vec<TaskRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, filename, destination, total_size, downloaded_bytes,
                    status, category, etag, created_at, updated_at, queue_id, queue_order
             FROM tasks ORDER BY queue_order ASC, created_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            let id_str: String = row.get(0)?;
            let total_size: Option<i64> = row.get(4)?;
            let downloaded: i64 = row.get(5)?;
            let status_str: String = row.get(6)?;
            let created_at: i64 = row.get(9)?;
            let updated_at: i64 = row.get(10)?;
            let queue_id: String = row.get(11).unwrap_or_else(|_| "main".to_string());
            let queue_order: i64 = row.get(12).unwrap_or(0);

            Ok(TaskRecord {
                id: Uuid::parse_str(&id_str).unwrap_or_default(),
                url: row.get(1)?,
                filename: row.get(2)?,
                destination: PathBuf::from(row.get::<_, String>(3)?),
                total_size: total_size.map(|s| s as u64),
                downloaded_bytes: downloaded as u64,
                status: TaskStatus::from_str(&status_str),
                category: row.get(7)?,
                etag: row.get(8)?,
                created_at: created_at as u64,
                updated_at: updated_at as u64,
                queue_id,
                queue_order,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_task(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM tasks WHERE id = ?1", params![id.to_string()])?;
        Ok(())
    }

    pub fn set_task_queue(&self, task_id: Uuid, queue_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET queue_id = ?1 WHERE id = ?2",
            params![queue_id, task_id.to_string()],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------
    // Queue System Operations
    // -----------------------------------------------------------------

    pub fn get_all_queues(&self) -> Result<Vec<QueueRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, max_concurrent, speed_limit_kbps, is_active, schedule_start, schedule_stop, created_at, on_complete_action
             FROM queues ORDER BY created_at ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            let is_active_int: i64 = row.get(4)?;
            Ok(QueueRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                max_concurrent: row.get::<_, i64>(2)? as usize,
                speed_limit_kbps: row.get::<_, i64>(3)? as u64,
                is_active: is_active_int != 0,
                schedule_start: row.get(5)?,
                schedule_stop: row.get(6)?,
                created_at: row.get::<_, i64>(7)? as u64,
                on_complete_action: row.get(8).unwrap_or(Some("none".to_string())),
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_queue(&self, id: &str) -> Result<Option<QueueRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, max_concurrent, speed_limit_kbps, is_active, schedule_start, schedule_stop, created_at, on_complete_action
             FROM queues WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let is_active_int: i64 = row.get(4)?;
            Ok(Some(QueueRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                max_concurrent: row.get::<_, i64>(2)? as usize,
                speed_limit_kbps: row.get::<_, i64>(3)? as u64,
                is_active: is_active_int != 0,
                schedule_start: row.get(5)?,
                schedule_stop: row.get(6)?,
                created_at: row.get::<_, i64>(7)? as u64,
                on_complete_action: row.get(8).unwrap_or(Some("none".to_string())),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn insert_or_update_queue(&self, q: &QueueRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO queues (
                id, name, max_concurrent, speed_limit_kbps, is_active, schedule_start, schedule_stop, created_at, on_complete_action
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                q.id,
                q.name,
                q.max_concurrent as i64,
                q.speed_limit_kbps as i64,
                if q.is_active { 1 } else { 0 },
                q.schedule_start,
                q.schedule_stop,
                q.created_at as i64,
                q.on_complete_action.as_deref().unwrap_or("none"),
            ],
        )?;
        Ok(())
    }

    pub fn delete_queue(&self, id: &str) -> Result<()> {
        if id == "main" {
            // Cannot delete default main queue
            return Ok(());
        }
        let conn = self.conn.lock().unwrap();
        // Reassign tasks from this deleted queue to default 'main'
        conn.execute("UPDATE tasks SET queue_id = 'main' WHERE queue_id = ?1", params![id])?;
        conn.execute("DELETE FROM queues WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn set_queue_active(&self, id: &str, is_active: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE queues SET is_active = ?1 WHERE id = ?2",
            params![if is_active { 1 } else { 0 }, id],
        )?;
        Ok(())
    }

    pub fn count_downloading_tasks_in_queue(&self, queue_id: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE queue_id = ?1 AND status = 'downloading'",
            params![queue_id],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn get_next_queued_tasks_in_queue(&self, queue_id: &str, limit: usize) -> Result<Vec<TaskRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, filename, destination, total_size, downloaded_bytes,
                    status, category, etag, created_at, updated_at, queue_id, queue_order
             FROM tasks WHERE queue_id = ?1 AND status = 'queued'
             ORDER BY queue_order ASC, created_at ASC LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![queue_id, limit as i64], |row| {
            let id_str: String = row.get(0)?;
            let total_size: Option<i64> = row.get(4)?;
            let downloaded: i64 = row.get(5)?;
            let status_str: String = row.get(6)?;
            let created_at: i64 = row.get(9)?;
            let updated_at: i64 = row.get(10)?;
            let q_id: String = row.get(11).unwrap_or_else(|_| "main".to_string());
            let queue_order: i64 = row.get(12).unwrap_or(0);

            Ok(TaskRecord {
                id: Uuid::parse_str(&id_str).unwrap_or_default(),
                url: row.get(1)?,
                filename: row.get(2)?,
                destination: PathBuf::from(row.get::<_, String>(3)?),
                total_size: total_size.map(|s| s as u64),
                downloaded_bytes: downloaded as u64,
                status: TaskStatus::from_str(&status_str),
                category: row.get(7)?,
                etag: row.get(8)?,
                created_at: created_at as u64,
                updated_at: updated_at as u64,
                queue_id: q_id,
                queue_order,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }
}
