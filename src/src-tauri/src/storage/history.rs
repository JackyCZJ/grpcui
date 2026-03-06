#![allow(dead_code)]
use super::{
    db::Database,
    error::{Result, StorageError},
    models::{CreateHistory, Filters, History, HistoryEntry, RequestItem, new_storage_id},
};
use serde_json;
use sqlx::Row;

pub struct HistoryStore<'a> {
    db: &'a Database,
}

impl<'a> HistoryStore<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub async fn add_history(&self, history: &CreateHistory) -> Result<History> {
        let id = new_storage_id();
        let now = chrono::Local::now().naive_local();
        let snapshot = serde_json::to_string(&history.request_snapshot)?;

        sqlx::query(
            "INSERT INTO history (id, project_id, timestamp, service, method, address, status, response_code, response_message, duration, request_snapshot, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&history.project_id)
        .bind(history.timestamp)
        .bind(&history.service)
        .bind(&history.method)
        .bind(&history.address)
        .bind(&history.status)
        .bind(history.response_code)
        .bind(&history.response_message)
        .bind(history.duration)
        .bind(&snapshot)
        .bind(now)
        .execute(self.db.pool())
        .await?;

        Ok(History {
            id,
            project_id: history.project_id.clone(),
            timestamp: history.timestamp,
            service: history.service.clone(),
            method: history.method.clone(),
            address: history.address.clone(),
            status: history.status.clone(),
            response_code: history.response_code,
            response_message: history.response_message.clone(),
            duration: history.duration,
            request_snapshot: history.request_snapshot.clone(),
            created_at: now,
        })
    }

    pub async fn get_history(&self, id: &str) -> Result<Option<History>> {
        let row = sqlx::query(
            "SELECT id, project_id, timestamp, service, method, address, status, response_code, response_message, duration, request_snapshot, created_at
             FROM history WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await?;

        match row {
            Some(row) => Ok(Some(self.row_to_history(row)?)),
            None => Ok(None),
        }
    }

    pub async fn list_histories(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<History>> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);

        let rows = sqlx::query(
            "SELECT id, project_id, timestamp, service, method, address, status, response_code, response_message, duration, request_snapshot, created_at
             FROM history ORDER BY timestamp DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await?;

        let mut histories = Vec::new();
        for row in rows {
            histories.push(self.row_to_history(row)?);
        }

        Ok(histories)
    }

    pub async fn search_history(&self, query: &str, filters: &Filters) -> Result<Vec<History>> {
        let mut conditions = Vec::new();
        let mut args: Vec<String> = Vec::new();

        if !query.is_empty() {
            conditions.push("(LOWER(service) LIKE ? OR LOWER(method) LIKE ?)".to_string());
            let pattern = format!("%{}%", query.to_lowercase());
            args.push(pattern.clone());
            args.push(pattern);
        }

        if let Some(service) = &filters.service {
            conditions.push("service = ?".to_string());
            args.push(service.clone());
        }

        if let Some(method) = &filters.method {
            conditions.push("method = ?".to_string());
            args.push(method.clone());
        }

        if let Some(status) = &filters.status {
            conditions.push("status = ?".to_string());
            args.push(status.clone());
        }

        if let Some(start_time) = filters.start_time {
            conditions.push("timestamp >= ?".to_string());
            args.push(start_time.to_string());
        }

        if let Some(end_time) = filters.end_time {
            conditions.push("timestamp <= ?".to_string());
            args.push(end_time.to_string());
        }

        let mut sql = String::from(
            "SELECT id, project_id, timestamp, service, method, address, status, response_code, response_message, duration, request_snapshot, created_at FROM history"
        );

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sql.push_str(" ORDER BY timestamp DESC");

        if let Some(limit) = filters.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
            if let Some(offset) = filters.offset {
                sql.push_str(&format!(" OFFSET {}", offset));
            }
        }

        let mut query = sqlx::query(&sql);

        for arg in args {
            query = query.bind(arg);
        }

        let rows = query.fetch_all(self.db.pool()).await?;

        let mut histories = Vec::new();
        for row in rows {
            histories.push(self.row_to_history(row)?);
        }

        Ok(histories)
    }

    pub async fn batch_insert_history(&self, entries: &[HistoryEntry]) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;

        for entry in entries {
            let id = new_storage_id();
            let now = chrono::Local::now().naive_local();
            let snapshot = serde_json::to_string(&entry.request_snapshot)?;

            sqlx::query(
                "INSERT INTO history (id, project_id, timestamp, service, method, address, status, response_code, response_message, duration, request_snapshot, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&entry.project_id)
            .bind(entry.timestamp)
            .bind(&entry.service)
            .bind(&entry.method)
            .bind(&entry.address)
            .bind(&entry.status)
            .bind(entry.response_code)
            .bind(&entry.response_message)
            .bind(entry.duration)
            .bind(&snapshot)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn delete_history(&self, id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM history WHERE id = ?")
            .bind(id)
            .execute(self.db.pool())
            .await?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("History {} not found", id)));
        }

        Ok(())
    }

    pub async fn clear_history(&self) -> Result<()> {
        sqlx::query("DELETE FROM history")
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// clear_history_by_project 按 project_id 清空指定项目的历史记录。
    ///
    /// 该方法用于前端“删除全部历史（当前项目）”操作，避免误删其他项目数据。
    /// 当目标项目暂无历史时返回 Ok，不把“0 行受影响”视为错误。
    pub async fn clear_history_by_project(&self, project_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM history WHERE project_id = ?")
            .bind(project_id)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    fn row_to_history(&self, row: sqlx::sqlite::SqliteRow) -> Result<History> {
        let snapshot_str: String = row.try_get("request_snapshot")?;
        let snapshot: RequestItem = serde_json::from_str(&snapshot_str).map_err(|_| {
            StorageError::SerializationError("Failed to parse request snapshot".to_string())
        })?;

        Ok(History {
            id: row.try_get("id")?,
            project_id: row.try_get("project_id").ok(),
            timestamp: row.try_get("timestamp")?,
            service: row.try_get("service")?,
            method: row.try_get("method")?,
            address: row.try_get("address")?,
            status: row.try_get("status")?,
            response_code: row.try_get("response_code")?,
            response_message: row.try_get("response_message")?,
            duration: row.try_get("duration")?,
            request_snapshot: snapshot,
            created_at: row.try_get("created_at")?,
        })
    }
}
