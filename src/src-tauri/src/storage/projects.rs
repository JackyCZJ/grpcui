use super::{
    db::Database,
    error::{Result, StorageError},
    models::{CreateProject, Project, UpdateProject, new_storage_id},
};
use serde_json;
use sqlx::{Row, Transaction};

pub struct ProjectStore<'a> {
    db: &'a Database,
}

impl<'a> ProjectStore<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub async fn create_project(&self, project: &CreateProject) -> Result<Project> {
        if project.name.trim().is_empty() {
            return Err(StorageError::InvalidInput(
                "Project name is required".to_string(),
            ));
        }

        let id = new_storage_id();
        let now = chrono::Local::now().naive_local();
        let proto_files = serde_json::to_string(&project.proto_files)?;

        sqlx::query(
            "INSERT INTO projects (id, name, description, default_environment_id, proto_files, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&project.name)
        .bind(&project.description)
        .bind::<Option<&str>>(None)
        .bind(&proto_files)
        .bind(now)
        .bind(now)
        .execute(self.db.pool())
        .await?;

        Ok(Project {
            id,
            name: project.name.clone(),
            description: project.description.clone(),
            default_environment_id: None,
            proto_files: project.proto_files.clone(),
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn get_project(&self, id: &str) -> Result<Option<Project>> {
        let row = sqlx::query(
            "SELECT id, name, description, default_environment_id, proto_files, created_at, updated_at
             FROM projects WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await?;

        match row {
            Some(row) => {
                let proto_files_str: String = row.try_get("proto_files").unwrap_or_default();
                let proto_files: Vec<String> =
                    serde_json::from_str(&proto_files_str).unwrap_or_default();

                Ok(Some(Project {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    description: row.try_get("description").unwrap_or_default(),
                    default_environment_id: row.try_get::<Option<String>, _>("default_environment_id")?,
                    proto_files,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let rows = sqlx::query(
            "SELECT id, name, description, default_environment_id, proto_files, created_at, updated_at
             FROM projects ORDER BY name",
        )
        .fetch_all(self.db.pool())
        .await?;

        let mut projects = Vec::new();
        for row in rows {
            let proto_files_str: String = row.try_get("proto_files").unwrap_or_default();
            let proto_files: Vec<String> =
                serde_json::from_str(&proto_files_str).unwrap_or_default();

            projects.push(Project {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                description: row.try_get("description").unwrap_or_default(),
                default_environment_id: row.try_get::<Option<String>, _>("default_environment_id")?,
                proto_files,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            });
        }

        Ok(projects)
    }

    pub async fn update_project(&self, id: &str, update: &UpdateProject) -> Result<Project> {
        let existing = self
            .get_project(id)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("Project {} not found", id)))?;

        let name = update.name.as_ref().unwrap_or(&existing.name);
        let description = update
            .description
            .as_ref()
            .unwrap_or(&existing.description);
        let proto_files = update
            .proto_files
            .as_ref()
            .unwrap_or(&existing.proto_files);
        let default_env_id = update
            .default_environment_id
            .as_ref()
            .or(existing.default_environment_id.as_ref());

        let now = chrono::Local::now().naive_local();
        let proto_files_json = serde_json::to_string(proto_files)?;

        sqlx::query(
            "UPDATE projects SET name = ?, description = ?, default_environment_id = ?, proto_files = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(name)
        .bind(description)
        .bind(default_env_id)
        .bind(&proto_files_json)
        .bind(now)
        .bind(id)
        .execute(self.db.pool())
        .await?;

        Ok(Project {
            id: id.to_string(),
            name: name.clone(),
            description: description.clone(),
            default_environment_id: default_env_id.cloned(),
            proto_files: proto_files.clone(),
            created_at: existing.created_at,
            updated_at: now,
        })
    }

    pub async fn delete_project(&self, id: &str) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;

        // Delete in order to respect foreign keys
        sqlx::query("DELETE FROM project_environments WHERE project_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM history WHERE project_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM collections WHERE project_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM environments WHERE project_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        let result = sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("Project {} not found", id)));
        }

        Ok(())
    }

    pub async fn clone_project(&self, id: &str, new_name: &str) -> Result<Project> {
        if new_name.trim().is_empty() {
            return Err(StorageError::InvalidInput(
                "New name is required".to_string(),
            ));
        }

        let source = self
            .get_project(id)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("Project {} not found", id)))?;

        let mut tx = self.db.pool().begin().await?;

        let new_id = new_storage_id();
        let now = chrono::Local::now().naive_local();
        let proto_files_json = serde_json::to_string(&source.proto_files)?;

        sqlx::query(
            "INSERT INTO projects (id, name, description, default_environment_id, proto_files, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&new_id)
        .bind(new_name)
        .bind(&source.description)
        .bind::<Option<&str>>(None)
        .bind(&proto_files_json)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        // Clone environments
        let env_id_map = self
            .clone_project_environments(&mut tx, id, &new_id)
            .await?;

        // Clone collections
        self.clone_project_collections(&mut tx, id, &new_id, &env_id_map)
            .await?;

        tx.commit().await?;

        // Update default environment if exists
        if let Some(old_default) = source.default_environment_id {
            if let Some(new_default) = env_id_map.get(&old_default) {
                sqlx::query(
                    "UPDATE projects SET default_environment_id = ? WHERE id = ?",
                )
                .bind(new_default)
                .bind(&new_id)
                .execute(self.db.pool())
                .await?;

                sqlx::query(
                    "UPDATE environments SET is_default = TRUE WHERE id = ? AND project_id = ?",
                )
                .bind(new_default)
                .bind(&new_id)
                .execute(self.db.pool())
                .await?;
            }
        }

        self.get_project(&new_id)
            .await?
            .ok_or_else(|| StorageError::DatabaseError("Failed to get cloned project".to_string()))
    }

    async fn clone_project_environments(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        source_project_id: &str,
        target_project_id: &str,
    ) -> Result<std::collections::HashMap<String, String>> {
        use sqlx::Row;

        let rows = sqlx::query(
            "SELECT id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at
             FROM environments WHERE project_id = ?",
        )
        .bind(source_project_id)
        .fetch_all(&mut **tx)
        .await?;

        let mut mapping = std::collections::HashMap::new();

        for row in rows {
            let old_id: String = row.try_get("id")?;
            let new_id = new_storage_id();
            mapping.insert(old_id.clone(), new_id.clone());

            let now = chrono::Local::now().naive_local();

            sqlx::query(
                "INSERT INTO environments (id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, FALSE, ?, ?)",
            )
            .bind(&new_id)
            .bind(target_project_id)
            .bind::<String>(row.try_get("name")?)
            .bind::<String>(row.try_get("base_url")?)
            .bind::<String>(row.try_get("variables")?)
            .bind::<String>(row.try_get("headers")?)
            .bind::<String>(row.try_get("tls_config")?)
            .bind(now)
            .bind(now)
            .execute(&mut **tx)
            .await?;

            sqlx::query(
                "INSERT INTO project_environments (project_id, environment_id, is_default)
                 VALUES (?, ?, FALSE)
                 ON CONFLICT(project_id, environment_id) DO UPDATE SET is_default = FALSE",
            )
            .bind(target_project_id)
            .bind(&new_id)
            .execute(&mut **tx)
            .await?;
        }

        Ok(mapping)
    }

    async fn clone_project_collections(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        source_project_id: &str,
        target_project_id: &str,
        env_id_map: &std::collections::HashMap<String, String>,
    ) -> Result<()> {
        use super::models::Collection;
        use sqlx::Row;

        let rows = sqlx::query("SELECT data FROM collections WHERE project_id = ?")
            .bind(source_project_id)
            .fetch_all(&mut **tx)
            .await?;

        for row in rows {
            let data: String = row.try_get("data")?;
            let mut collection: Collection = serde_json::from_str(&data)?;

            let now = chrono::Local::now().naive_local();
            collection.id = new_storage_id();
            collection.project_id = target_project_id.to_string();
            collection.created_at = now;
            collection.updated_at = now;
            collection.items = clone_request_items(collection.items, env_id_map);

            for folder in &mut collection.folders {
                folder.id = new_storage_id();
                folder.items = clone_request_items(folder.items.clone(), env_id_map);
            }

            let collection_data = serde_json::to_string(&collection)?;

            sqlx::query(
                "INSERT INTO collections (id, project_id, name, data, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&collection.id)
            .bind(&collection.project_id)
            .bind(&collection.name)
            .bind(&collection_data)
            .bind(now)
            .bind(now)
            .execute(&mut **tx)
            .await?;
        }

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
}

fn clone_request_items(
    items: Vec<super::models::RequestItem>,
    env_id_map: &std::collections::HashMap<String, String>,
) -> Vec<super::models::RequestItem> {
    items
        .into_iter()
        .map(|mut item| {
            item.id = new_storage_id();
            if let Some(ref env_id) = item.environment_id {
                if let Some(mapped) = env_id_map.get(env_id) {
                    item.environment_id = Some(mapped.clone());
                } else {
                    item.environment_id = None;
                }
            }
            item
        })
        .collect()
}

use sqlx::Sqlite;
