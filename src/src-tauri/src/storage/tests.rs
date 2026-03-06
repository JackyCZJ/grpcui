#[cfg(test)]
mod storage_tests {
    use super::super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    async fn setup_test_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
        (db, temp_dir)
    }

    #[tokio::test]
    async fn test_project_crud() {
        let (db, _temp) = setup_test_db().await;
        let store = ProjectStore::new(&db);

        // Create
        let create = CreateProject {
            name: "Test Project".to_string(),
            description: "Test Description".to_string(),
            proto_files: vec!["test.proto".to_string()],
        };
        let project = store.create_project(&create).await.unwrap();
        assert_eq!(project.name, "Test Project");
        assert_eq!(project.description, "Test Description");
        assert_eq!(project.proto_files, vec!["test.proto"]);

        // Get
        let fetched = store.get_project(&project.id).await.unwrap().unwrap();
        assert_eq!(fetched.name, project.name);

        // List
        let projects = store.list_projects().await.unwrap();
        assert_eq!(projects.len(), 1);

        // Update
        let update = UpdateProject {
            name: Some("Updated Project".to_string()),
            description: Some("Updated Description".to_string()),
            proto_files: Some(vec!["updated.proto".to_string()]),
            default_environment_id: None,
        };
        let updated = store.update_project(&project.id, &update).await.unwrap();
        assert_eq!(updated.name, "Updated Project");

        // Delete
        store.delete_project(&project.id).await.unwrap();
        let deleted = store.get_project(&project.id).await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_environment_crud() {
        let (db, _temp) = setup_test_db().await;
        let env_store = EnvironmentStore::new(&db);
        let project_store = ProjectStore::new(&db);

        // Create project first
        let project = project_store
            .create_project(&CreateProject {
                name: "Test Project".to_string(),
                description: "".to_string(),
                proto_files: vec![],
            })
            .await
            .unwrap();

        // Create environment
        let mut variables = HashMap::new();
        variables.insert("key".to_string(), "value".to_string());

        let create = CreateEnvironment {
            project_id: project.id.clone(),
            name: "Test Environment".to_string(),
            base_url: "localhost:50051".to_string(),
            variables: variables.clone(),
            headers: HashMap::new(),
            tls_config: None,
            is_default: false,
        };
        let env = env_store.create_environment(&create).await.unwrap();
        assert_eq!(env.name, "Test Environment");
        assert_eq!(env.base_url, "localhost:50051");
        assert_eq!(env.variables, variables);

        // Get
        let fetched = env_store.get_environment(&env.id).await.unwrap().unwrap();
        assert_eq!(fetched.name, env.name);

        // List by project
        let envs = env_store
            .list_environments_by_project(&project.id)
            .await
            .unwrap();
        assert_eq!(envs.len(), 1);

        // Update
        let update = UpdateEnvironment {
            name: Some("Updated Environment".to_string()),
            base_url: Some("localhost:50052".to_string()),
            variables: None,
            headers: None,
            tls_config: None,
            is_default: None,
        };
        let updated = env_store.update_environment(&env.id, &update).await.unwrap();
        assert_eq!(updated.name, "Updated Environment");
        assert_eq!(updated.base_url, "localhost:50052");

        // 先设为项目默认环境，再验证删除后会自动清理项目默认引用
        project_store
            .set_default_environment(&project.id, &env.id)
            .await
            .unwrap();

        let project_with_default = project_store
            .get_project(&project.id)
            .await
            .unwrap()
            .expect("project should exist");
        assert_eq!(
            project_with_default.default_environment_id.as_deref(),
            Some(env.id.as_str())
        );

        // Delete
        env_store.delete_environment(&env.id).await.unwrap();
        let deleted = env_store.get_environment(&env.id).await.unwrap();
        assert!(deleted.is_none());

        let refreshed_project = project_store
            .get_project(&project.id)
            .await
            .unwrap()
            .expect("project should exist");
        assert!(refreshed_project.default_environment_id.is_none());
    }

    #[tokio::test]
    async fn test_collection_crud() {
        let (db, _temp) = setup_test_db().await;
        let col_store = CollectionStore::new(&db);
        let project_store = ProjectStore::new(&db);

        // Create project first
        let project = project_store
            .create_project(&CreateProject {
                name: "Test Project".to_string(),
                description: "".to_string(),
                proto_files: vec![],
            })
            .await
            .unwrap();

        // Create collection
        let item = RequestItem {
            id: "item-1".to_string(),
            name: "Test Request".to_string(),
            item_type: "unary".to_string(),
            service: "TestService".to_string(),
            method: "TestMethod".to_string(),
            body: "{}".to_string(),
            metadata: HashMap::new(),
            env_ref_type: None,
            environment_id: None,
        };

        let create = CreateCollection {
            project_id: project.id.clone(),
            name: "Test Collection".to_string(),
            folders: vec![],
            items: vec![item],
        };
        let collection = col_store.create_collection(&create).await.unwrap();
        assert_eq!(collection.name, "Test Collection");
        assert_eq!(collection.items.len(), 1);

        // Get
        let fetched = col_store.get_collection(&collection.id).await.unwrap().unwrap();
        assert_eq!(fetched.name, collection.name);

        // List by project
        let cols = col_store
            .list_collections_by_project(&project.id)
            .await
            .unwrap();
        assert_eq!(cols.len(), 1);

        // Update
        let update = UpdateCollection {
            name: Some("Updated Collection".to_string()),
            folders: None,
            items: None,
        };
        let updated = col_store
            .update_collection(&collection.id, &update)
            .await
            .unwrap();
        assert_eq!(updated.name, "Updated Collection");

        // Delete
        col_store.delete_collection(&collection.id).await.unwrap();
        let deleted = col_store.get_collection(&collection.id).await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_history_crud() {
        let (db, _temp) = setup_test_db().await;
        let hist_store = HistoryStore::new(&db);
        let project_store = ProjectStore::new(&db);

        // Create project first
        let project = project_store
            .create_project(&CreateProject {
                name: "Test Project".to_string(),
                description: "".to_string(),
                proto_files: vec![],
            })
            .await
            .unwrap();

        // Create history entry
        let snapshot = RequestItem {
            id: "req-1".to_string(),
            name: "Test Request".to_string(),
            item_type: "unary".to_string(),
            service: "TestService".to_string(),
            method: "TestMethod".to_string(),
            body: "{}".to_string(),
            metadata: HashMap::new(),
            env_ref_type: None,
            environment_id: None,
        };

        let create = CreateHistory {
            project_id: Some(project.id.clone()),
            timestamp: chrono::Local::now().timestamp_millis(),
            service: "TestService".to_string(),
            method: "TestMethod".to_string(),
            address: "localhost:50051".to_string(),
            status: "success".to_string(),
            response_code: Some(0),
            response_message: Some("OK".to_string()),
            duration: 100,
            request_snapshot: snapshot,
        };
        let history = hist_store.add_history(&create).await.unwrap();
        assert_eq!(history.service, "TestService");
        assert_eq!(history.status, "success");
        assert_eq!(history.response_code, Some(0));
        assert_eq!(history.response_message.as_deref(), Some("OK"));

        // List
        let histories = hist_store.list_histories(Some(10), Some(0)).await.unwrap();
        assert_eq!(histories.len(), 1);

        // Delete
        hist_store.delete_history(&history.id).await.unwrap();
    }

    #[tokio::test]
    async fn test_project_clone() {
        let (db, _temp) = setup_test_db().await;
        let store = ProjectStore::new(&db);
        let env_store = EnvironmentStore::new(&db);

        // Create project
        let project = store
            .create_project(&CreateProject {
                name: "Original Project".to_string(),
                description: "Original Description".to_string(),
                proto_files: vec!["test.proto".to_string()],
            })
            .await
            .unwrap();

        // Create environment
        env_store
            .create_environment(&CreateEnvironment {
                project_id: project.id.clone(),
                name: "Test Environment".to_string(),
                base_url: "localhost:50051".to_string(),
                variables: HashMap::new(),
                headers: HashMap::new(),
                tls_config: None,
                is_default: false,
            })
            .await
            .unwrap();

        // Clone project
        let cloned = store.clone_project(&project.id, "Cloned Project").await.unwrap();
        assert_eq!(cloned.name, "Cloned Project");
        assert_eq!(cloned.description, project.description);
        assert_ne!(cloned.id, project.id);

        // Verify cloned environments
        let cloned_envs = env_store
            .list_environments_by_project(&cloned.id)
            .await
            .unwrap();
        assert_eq!(cloned_envs.len(), 1);
        assert_eq!(cloned_envs[0].name, "Test Environment");
    }

    #[tokio::test]
    async fn test_set_default_environment() {
        let (db, _temp) = setup_test_db().await;
        let store = ProjectStore::new(&db);
        let env_store = EnvironmentStore::new(&db);

        // Create project
        let project = store
            .create_project(&CreateProject {
                name: "Test Project".to_string(),
                description: "".to_string(),
                proto_files: vec![],
            })
            .await
            .unwrap();

        // Create two environments
        let env1 = env_store
            .create_environment(&CreateEnvironment {
                project_id: project.id.clone(),
                name: "Environment 1".to_string(),
                base_url: "localhost:50051".to_string(),
                variables: HashMap::new(),
                headers: HashMap::new(),
                tls_config: None,
                is_default: false,
            })
            .await
            .unwrap();

        let env2 = env_store
            .create_environment(&CreateEnvironment {
                project_id: project.id.clone(),
                name: "Environment 2".to_string(),
                base_url: "localhost:50052".to_string(),
                variables: HashMap::new(),
                headers: HashMap::new(),
                tls_config: None,
                is_default: false,
            })
            .await
            .unwrap();

        // Set env1 as default
        store
            .set_default_environment(&project.id, &env1.id)
            .await
            .unwrap();

        let env1_fetched = env_store.get_environment(&env1.id).await.unwrap().unwrap();
        assert!(env1_fetched.is_default);

        // Set env2 as default
        store
            .set_default_environment(&project.id, &env2.id)
            .await
            .unwrap();

        let env1_fetched = env_store.get_environment(&env1.id).await.unwrap().unwrap();
        let env2_fetched = env_store.get_environment(&env2.id).await.unwrap().unwrap();
        assert!(!env1_fetched.is_default);
        assert!(env2_fetched.is_default);
    }
}
