#![allow(dead_code)]
use super::{
    db::Database,
    error::{Result, StorageError},
    models::{CreateEnvironment, Environment, TLSConfig, UpdateEnvironment, new_storage_id},
};
use serde_json;
use sqlx::Row;

pub struct EnvironmentStore<'a> {
    db: &'a Database,
}

impl<'a> EnvironmentStore<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub async fn create_environment(&self, env: &CreateEnvironment) -> Result<Environment> {
        if env.name.trim().is_empty() {
            return Err(StorageError::InvalidInput(
                "Environment name is required".to_string(),
            ));
        }

        let id = new_storage_id();
        let now = chrono::Local::now().naive_local();
        let variables = serde_json::to_string(&env.variables)?;
        let headers = serde_json::to_string(&env.headers)?;
        let tls_config = serde_json::to_string(&env.tls_config)?;

        sqlx::query(
            "INSERT INTO environments (id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&env.project_id)
        .bind(&env.name)
        .bind(&env.base_url)
        .bind(&variables)
        .bind(&headers)
        .bind(&tls_config)
        .bind(env.is_default)
        .bind(now)
        .bind(now)
        .execute(self.db.pool())
        .await?;

        // Insert into project_environments
        sqlx::query(
            "INSERT INTO project_environments (project_id, environment_id, is_default)
             VALUES (?, ?, ?)
             ON CONFLICT(project_id, environment_id) DO UPDATE SET is_default = excluded.is_default",
        )
        .bind(&env.project_id)
        .bind(&id)
        .bind(env.is_default)
        .execute(self.db.pool())
        .await?;

        // If this is default, update other environments
        if env.is_default {
            self.set_default_environment(&env.project_id, &id).await?;
        }

        Ok(Environment {
            id,
            project_id: env.project_id.clone(),
            name: env.name.clone(),
            base_url: env.base_url.clone(),
            variables: env.variables.clone(),
            headers: env.headers.clone(),
            tls_config: env.tls_config.clone(),
            is_default: env.is_default,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn get_environment(&self, id: &str) -> Result<Option<Environment>> {
        let row = sqlx::query(
            "SELECT id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at
             FROM environments WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await?;

        match row {
            Some(row) => Ok(Some(self.row_to_environment(row)?)),
            None => Ok(None),
        }
    }

    pub async fn list_environments(&self) -> Result<Vec<Environment>> {
        let rows = sqlx::query(
            "SELECT id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at
             FROM environments ORDER BY name",
        )
        .fetch_all(self.db.pool())
        .await?;

        let mut envs = Vec::new();
        for row in rows {
            envs.push(self.row_to_environment(row)?);
        }

        Ok(envs)
    }

    pub async fn list_environments_by_project(&self, project_id: &str) -> Result<Vec<Environment>> {
        let rows = sqlx::query(
            "SELECT id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at
             FROM environments WHERE project_id = ? ORDER BY is_default DESC, name",
        )
        .bind(project_id)
        .fetch_all(self.db.pool())
        .await?;

        let mut envs = Vec::new();
        for row in rows {
            envs.push(self.row_to_environment(row)?);
        }

        Ok(envs)
    }

    pub async fn update_environment(
        &self,
        id: &str,
        update: &UpdateEnvironment,
    ) -> Result<Environment> {
        let existing = self
            .get_environment(id)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("Environment {} not found", id)))?;

        let name = update.name.as_ref().unwrap_or(&existing.name);
        let base_url = update.base_url.as_ref().unwrap_or(&existing.base_url);
        let variables = update
            .variables
            .as_ref()
            .unwrap_or(&existing.variables);
        let headers = update.headers.as_ref().unwrap_or(&existing.headers);
        let tls_config = update
            .tls_config
            .as_ref()
            .and_then(|t| t.as_ref())
            .or(existing.tls_config.as_ref());
        let is_default = update.is_default.unwrap_or(existing.is_default);

        let now = chrono::Local::now().naive_local();
        let variables_json = serde_json::to_string(variables)?;
        let headers_json = serde_json::to_string(headers)?;
        let tls_config_json = serde_json::to_string(&tls_config)?;

        sqlx::query(
            "UPDATE environments SET name = ?, base_url = ?, variables = ?, headers = ?, tls_config = ?, is_default = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(name)
        .bind(base_url)
        .bind(&variables_json)
        .bind(&headers_json)
        .bind(&tls_config_json)
        .bind(is_default)
        .bind(now)
        .bind(id)
        .execute(self.db.pool())
        .await?;

        // Update project_environments
        sqlx::query(
            "INSERT INTO project_environments (project_id, environment_id, is_default)
             VALUES (?, ?, ?)
             ON CONFLICT(project_id, environment_id) DO UPDATE SET is_default = excluded.is_default",
        )
        .bind(&existing.project_id)
        .bind(id)
        .bind(is_default)
        .execute(self.db.pool())
        .await?;

        if is_default {
            self.set_default_environment(&existing.project_id, id).await?;
        }

        Ok(Environment {
            id: id.to_string(),
            project_id: existing.project_id,
            name: name.clone(),
            base_url: base_url.clone(),
            variables: variables.clone(),
            headers: headers.clone(),
            tls_config: tls_config.cloned(),
            is_default,
            created_at: existing.created_at,
            updated_at: now,
        })
    }

    // delete_environment 负责删除环境及其项目关联，
    // 若删除的是项目默认环境，还会同步清空 projects.default_environment_id，避免悬挂引用。
    pub async fn delete_environment(&self, id: &str) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;

        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM environments WHERE id = ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

        if exists == 0 {
            return Err(StorageError::NotFound(format!(
                "Environment {} not found",
                id
            )));
        }

        sqlx::query("DELETE FROM project_environments WHERE environment_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM environments WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        let now = chrono::Local::now().naive_local();
        sqlx::query(
            "UPDATE projects SET default_environment_id = NULL, updated_at = ? WHERE default_environment_id = ?",
        )
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn set_default_environment(
        &self,
        project_id: &str,
        env_id: &str,
    ) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;

        // Verify environment belongs to project
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM environments WHERE id = ? AND project_id = ?",
        )
        .bind(env_id)
        .bind(project_id)
        .fetch_one(&mut *tx)
        .await?;

        if count == 0 {
            return Err(StorageError::NotFound(
                "Environment not found in project".to_string(),
            ));
        }

        let now = chrono::Local::now().naive_local();

        // Clear default for all environments in project
        sqlx::query("UPDATE environments SET is_default = FALSE WHERE project_id = ?")
            .bind(project_id)
            .execute(&mut *tx)
            .await?;

        // Set new default
        sqlx::query(
            "UPDATE environments SET is_default = TRUE, updated_at = ? WHERE id = ? AND project_id = ?",
        )
        .bind(now)
        .bind(env_id)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

        // Update project_environments
        sqlx::query(
            "UPDATE project_environments SET is_default = FALSE WHERE project_id = ?",
        )
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO project_environments (project_id, environment_id, is_default)
             VALUES (?, ?, TRUE)
             ON CONFLICT(project_id, environment_id) DO UPDATE SET is_default = TRUE",
        )
        .bind(project_id)
        .bind(env_id)
        .execute(&mut *tx)
        .await?;

        // Update project
        sqlx::query(
            "UPDATE projects SET default_environment_id = ?, updated_at = ? WHERE id = ?",
        )
        .bind(env_id)
        .bind(now)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn search_environments(&self, query: &str) -> Result<Vec<Environment>> {
        let search_pattern = format!("%{}%", query.to_lowercase());

        let rows = sqlx::query(
            "SELECT id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at
             FROM environments
             WHERE LOWER(name) LIKE ? OR LOWER(base_url) LIKE ?
             ORDER BY name",
        )
        .bind(&search_pattern)
        .bind(&search_pattern)
        .fetch_all(self.db.pool())
        .await?;

        let mut envs = Vec::new();
        for row in rows {
            envs.push(self.row_to_environment(row)?);
        }

        Ok(envs)
    }

    fn row_to_environment(&self, row: sqlx::sqlite::SqliteRow) -> Result<Environment> {
        let variables_str: String = row.try_get("variables").unwrap_or_default();
        let headers_str: String = row.try_get("headers").unwrap_or_default();
        let tls_config_str: String = row.try_get("tls_config").unwrap_or_default();

        let variables: std::collections::HashMap<String, String> =
            serde_json::from_str(&variables_str).unwrap_or_default();
        let headers: std::collections::HashMap<String, String> =
            serde_json::from_str(&headers_str).unwrap_or_default();
        let tls_config: Option<TLSConfig> = serde_json::from_str(&tls_config_str).ok();

        Ok(Environment {
            id: row.try_get("id")?,
            project_id: row.try_get("project_id")?,
            name: row.try_get("name")?,
            base_url: row.try_get("base_url")?,
            variables,
            headers,
            tls_config,
            is_default: row.try_get("is_default").unwrap_or(false),
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}
