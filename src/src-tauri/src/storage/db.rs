use sqlx::{sqlite::SqliteConnectOptions, Pool, Sqlite, SqlitePool};
use std::path::Path;

use super::error::{Result, StorageError};

pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    pub async fn new(db_path: &str) -> Result<Self> {
        let path = Path::new(db_path);
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_millis(5000))
            .foreign_keys(true)
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options).await.map_err(|e| {
            StorageError::DatabaseError(format!("Failed to connect to database: {}", e))
        })?;

        let db = Self { pool };
        db.migrate().await?;

        Ok(db)
    }

    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    async fn migrate(&self) -> Result<()> {
        let schema = r#"
            -- Projects table
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                default_environment_id TEXT,
                proto_files TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_projects_name ON projects(name);

            -- Environments table
            CREATE TABLE IF NOT EXISTS environments (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                name TEXT NOT NULL,
                base_url TEXT NOT NULL,
                variables TEXT,
                headers TEXT,
                tls_config TEXT,
                is_default BOOLEAN DEFAULT FALSE,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_environments_name ON environments(name);

            -- Project-Environments association table
            CREATE TABLE IF NOT EXISTS project_environments (
                project_id TEXT NOT NULL,
                environment_id TEXT NOT NULL,
                is_default BOOLEAN DEFAULT FALSE,
                PRIMARY KEY (project_id, environment_id),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
                FOREIGN KEY (environment_id) REFERENCES environments(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_proj_env_project ON project_environments(project_id);
            CREATE INDEX IF NOT EXISTS idx_proj_env_env ON project_environments(environment_id);

            -- Collections table
            CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                name TEXT NOT NULL,
                data TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_collections_name ON collections(name);

            -- History table
            CREATE TABLE IF NOT EXISTS history (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                timestamp INTEGER NOT NULL,
                service TEXT NOT NULL,
                method TEXT NOT NULL,
                address TEXT NOT NULL,
                status TEXT NOT NULL,
                response_code INTEGER,
                response_message TEXT,
                duration INTEGER NOT NULL,
                request_snapshot TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_history_timestamp ON history(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_history_service ON history(service);
            CREATE INDEX IF NOT EXISTS idx_history_method ON history(method);
            CREATE INDEX IF NOT EXISTS idx_history_status ON history(status);
        "#;

        sqlx::query(schema)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::MigrationError(format!("Failed to create schema: {}", e)))?;

        self.ensure_schema_columns().await?;
        self.ensure_project_scoped_indexes().await?;
        self.migrate_existing_data().await?;

        Ok(())
    }

    async fn ensure_schema_columns(&self) -> Result<()> {
        let columns = vec![
            ("environments", "project_id", "project_id TEXT"),
            ("environments", "is_default", "is_default BOOLEAN DEFAULT FALSE"),
            ("collections", "project_id", "project_id TEXT"),
            ("history", "project_id", "project_id TEXT"),
            ("history", "response_code", "response_code INTEGER"),
            ("history", "response_message", "response_message TEXT"),
        ];

        for (table, column, definition) in columns {
            let has_column = self.has_column(table, column).await?;
            if !has_column {
                let query = format!("ALTER TABLE {} ADD COLUMN {}", table, definition);
                sqlx::query(&query)
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        StorageError::MigrationError(format!(
                            "Failed to add {}.{}: {}",
                            table, column, e
                        ))
                    })?;
            }
        }

        Ok(())
    }

    async fn ensure_project_scoped_indexes(&self) -> Result<()> {
        let statements = vec![
            "CREATE INDEX IF NOT EXISTS idx_environments_project ON environments(project_id)",
            "CREATE INDEX IF NOT EXISTS idx_collections_project ON collections(project_id)",
            "CREATE INDEX IF NOT EXISTS idx_history_project ON history(project_id)",
        ];

        for statement in statements {
            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .map_err(|e| StorageError::MigrationError(e.to_string()))?;
        }

        Ok(())
    }

    async fn has_column(&self, table: &str, column: &str) -> Result<bool> {
        let query = format!("PRAGMA table_info({})", table);
        let rows: Vec<(i64, String, String, i64, Option<String>, i64)> =
            sqlx::query_as(&query).fetch_all(&self.pool).await?;

        for row in rows {
            if row.1 == column {
                return Ok(true);
            }
        }

        Ok(false)
    }

    async fn migrate_existing_data(&self) -> Result<()> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM environments WHERE project_id IS NULL")
            .fetch_one(&self.pool)
            .await?;

        if count == 0 {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        let default_project_id = "default-project";
        let now = chrono::Local::now().naive_local();

        sqlx::query(
            "INSERT OR IGNORE INTO projects (id, name, description, proto_files, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(default_project_id)
        .bind("Default Project")
        .bind("Auto-created project for existing data")
        .bind("[]")
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        sqlx::query("UPDATE environments SET project_id = ? WHERE project_id IS NULL")
            .bind(default_project_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("UPDATE collections SET project_id = ? WHERE project_id IS NULL")
            .bind(default_project_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("UPDATE history SET project_id = ? WHERE project_id IS NULL")
            .bind(default_project_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }
}
