// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod error;
mod ffi;
mod grpc;
mod stream_manager;
mod storage;

use std::path::PathBuf;
use std::sync::Arc;
use stream_manager::StreamManager;
use storage::Database;

pub struct AppState {
    db: Arc<Database>,
    ffi: Arc<ffi::FFIBridge>,
}

/// 解析并返回 SQLite 数据库路径。
///
/// 优先读取 `GRPCUI_DB_PATH` 环境变量，这样开发时可以把数据库放到项目目录外，
/// 避免 `grpcui.db-shm/grpcui.db-wal` 写入触发 `tauri dev` 文件监听导致应用反复重启。
///
/// 若环境变量未设置或为空，则回退到默认值 `grpcui.db`（保持现有行为兼容）。
/// 同时在使用自定义路径时会尝试创建父目录，降低首次启动失败概率。
fn resolve_db_path() -> String {
    if let Ok(raw_path) = std::env::var("GRPCUI_DB_PATH") {
        let trimmed = raw_path.trim();
        if !trimmed.is_empty() {
            let db_path = PathBuf::from(trimmed);
            if let Some(parent) = db_path.parent() {
                if !parent.as_os_str().is_empty() {
                    if let Err(err) = std::fs::create_dir_all(parent) {
                        eprintln!(
                            "Warning: failed to create database parent directory {}: {}",
                            parent.display(),
                            err
                        );
                    }
                }
            }
            return db_path.to_string_lossy().into_owned();
        }
    }

    "grpcui.db".to_string()
}

#[tokio::main]
async fn main() {
    // Initialize logging
    if cfg!(debug_assertions) {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    }

    // Initialize SQLite database
    let db_path = resolve_db_path();
    let db = match Database::new(&db_path).await {
        Ok(db) => Arc::new(db),
        Err(e) => {
            eprintln!("Failed to initialize database at {}: {}", db_path, e);
            std::process::exit(1);
        }
    };

    // Initialize FFI bridge (loads Go shared library)
    let ffi = match ffi::FFIBridge::new() {
        Ok(ffi) => Arc::new(ffi),
        Err(e) => {
            eprintln!("Failed to initialize FFI bridge: {}", e);
            std::process::exit(1);
        }
    };

    let state = AppState { db, ffi };
    let stream_manager = Arc::new(StreamManager::new());

    tauri::Builder::default()
        .manage(state)
        .manage(stream_manager)
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::discover_proto_files,
            commands::grpc_connect,
            commands::grpc_disconnect,
            commands::grpc_list_services,
            commands::grpc_get_method_input_schema,
            commands::grpc_invoke,
            commands::grpc_invoke_stream,
            commands::grpc_send_stream_message,
            commands::grpc_end_stream,
            commands::grpc_close_stream,
            commands::save_environment,
            commands::delete_environment,
            commands::get_environments,
            commands::get_environments_by_project,
            commands::save_collection,
            commands::get_collections,
            commands::get_collections_by_project,
            commands::get_projects,
            commands::create_project,
            commands::update_project,
            commands::delete_project,
            commands::clone_project,
            commands::set_default_environment,
            commands::add_history,
            commands::get_histories,
            commands::delete_history_command,
            commands::clear_histories_command,
        ])
        .setup(|_app| {
            log::info!("gRPC UI application started with FFI bridge");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::resolve_db_path;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_db_path_uses_default_when_env_absent() {
        let _guard = ENV_LOCK.lock().expect("failed to lock env mutex");
        std::env::remove_var("GRPCUI_DB_PATH");

        assert_eq!(resolve_db_path(), "grpcui.db");
    }

    #[test]
    fn resolve_db_path_uses_env_value_when_present() {
        let _guard = ENV_LOCK.lock().expect("failed to lock env mutex");
        let expected = format!(
            "{}/grpcui-dev-tests/grpcui.db",
            std::env::temp_dir().to_string_lossy()
        );
        std::env::set_var("GRPCUI_DB_PATH", &expected);

        let resolved = resolve_db_path();
        assert_eq!(resolved, expected);

        std::env::remove_var("GRPCUI_DB_PATH");
    }
}
