#![allow(dead_code)]
use super::{
    db::Database,
    error::{Result, StorageError},
    models::{Collection, CreateCollection, UpdateCollection, new_storage_id},
};
use serde_json;
use sqlx::Row;

pub struct CollectionStore<'a> {
    db: &'a Database,
}

impl<'a> CollectionStore<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub async fn create_collection(&self, collection: &CreateCollection) -> Result<Collection> {
        if collection.name.trim().is_empty() {
            return Err(StorageError::InvalidInput(
                "Collection name is required".to_string(),
            ));
        }

        let id = new_storage_id();
        let now = chrono::Local::now().naive_local();

        let collection_data = Collection {
            id: id.clone(),
            project_id: collection.project_id.clone(),
            name: collection.name.clone(),
            folders: collection.folders.clone(),
            items: collection.items.clone(),
            created_at: now,
            updated_at: now,
        };

        let data = serde_json::to_string(&collection_data)?;

        sqlx::query(
            "INSERT INTO collections (id, project_id, name, data, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&collection.project_id)
        .bind(&collection.name)
        .bind(&data)
        .bind(now)
        .bind(now)
        .execute(self.db.pool())
        .await?;

        Ok(collection_data)
    }

    pub async fn get_collection(&self, id: &str) -> Result<Option<Collection>> {
        let row = sqlx::query("SELECT data FROM collections WHERE id = ?")
            .bind(id)
            .fetch_optional(self.db.pool())
            .await?;

        match row {
            Some(row) => {
                let data: String = row.try_get("data")?;
                let mut collection: Collection = serde_json::from_str(&data)?;

                // Ensure project_id is set
                if collection.project_id.is_empty() {
                    let project_row =
                        sqlx::query("SELECT project_id FROM collections WHERE id = ?")
                            .bind(id)
                            .fetch_one(self.db.pool())
                            .await?;
                    collection.project_id = project_row.try_get("project_id").unwrap_or_default();
                }

                Ok(Some(collection))
            }
            None => Ok(None),
        }
    }

    pub async fn list_collections(&self) -> Result<Vec<Collection>> {
        let rows = sqlx::query("SELECT project_id, data FROM collections ORDER BY name")
            .fetch_all(self.db.pool())
            .await?;

        let mut collections = Vec::new();
        for row in rows {
            let project_id: Option<String> = row.try_get("project_id").ok();
            let data: String = row.try_get("data")?;
            let mut collection: Collection = serde_json::from_str(&data)?;

            // Ensure project_id is set from DB if missing in JSON
            if collection.project_id.is_empty() {
                if let Some(pid) = project_id {
                    collection.project_id = pid;
                }
            }

            collections.push(collection);
        }

        Ok(collections)
    }

    pub async fn list_collections_by_project(&self, project_id: &str) -> Result<Vec<Collection>> {
        let rows = sqlx::query(
            "SELECT project_id, data FROM collections WHERE project_id = ? ORDER BY name",
        )
        .bind(project_id)
        .fetch_all(self.db.pool())
        .await?;

        let mut collections = Vec::new();
        for row in rows {
            let db_project_id: Option<String> = row.try_get("project_id").ok();
            let data: String = row.try_get("data")?;
            let mut collection: Collection = serde_json::from_str(&data)?;

            // Ensure project_id is set
            if collection.project_id.is_empty() {
                if let Some(pid) = db_project_id {
                    collection.project_id = pid;
                } else {
                    collection.project_id = project_id.to_string();
                }
            }

            collections.push(collection);
        }

        Ok(collections)
    }

    pub async fn update_collection(
        &self,
        id: &str,
        update: &UpdateCollection,
    ) -> Result<Collection> {
        let existing = self
            .get_collection(id)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("Collection {} not found", id)))?;

        let name = update.name.as_ref().unwrap_or(&existing.name);
        let folders = update.folders.as_ref().unwrap_or(&existing.folders);
        let items = update.items.as_ref().unwrap_or(&existing.items);

        let now = chrono::Local::now().naive_local();

        let updated_collection = Collection {
            id: id.to_string(),
            project_id: existing.project_id.clone(),
            name: name.clone(),
            folders: folders.clone(),
            items: items.clone(),
            created_at: existing.created_at,
            updated_at: now,
        };

        let data = serde_json::to_string(&updated_collection)?;

        sqlx::query(
            "UPDATE collections SET name = ?, data = ?, updated_at = ? WHERE id = ?",
        )
        .bind(name)
        .bind(&data)
        .bind(now)
        .bind(id)
        .execute(self.db.pool())
        .await?;

        Ok(updated_collection)
    }

    pub async fn delete_collection(&self, id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM collections WHERE id = ?")
            .bind(id)
            .execute(self.db.pool())
            .await?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!(
                "Collection {} not found",
                id
            )));
        }

        Ok(())
    }

    pub async fn export_collections(&self) -> Result<Vec<u8>> {
        let collections = self.list_collections().await?;
        let json = serde_json::to_vec_pretty(&collections)?;
        Ok(json)
    }

    pub async fn import_collections(&self, data: &[u8]) -> Result<()> {
        let collections: Vec<Collection> = serde_json::from_slice(data)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;

        let mut tx = self.db.pool().begin().await?;

        for collection in collections {
            let now = chrono::Local::now().naive_local();
            let data = serde_json::to_string(&collection)?;

            sqlx::query(
                "INSERT INTO collections (id, project_id, name, data, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(id) DO UPDATE SET
                 project_id = excluded.project_id,
                 name = excluded.name,
                 data = excluded.data,
                 updated_at = excluded.updated_at",
            )
            .bind(&collection.id)
            .bind(&collection.project_id)
            .bind(&collection.name)
            .bind(&data)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
