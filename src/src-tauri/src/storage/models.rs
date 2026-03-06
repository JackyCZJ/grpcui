#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "default_environment_id", default)]
    pub default_environment_id: Option<String>,
    #[serde(default)]
    pub proto_files: Vec<String>,
    #[serde(with = "chrono_datetime", default)]
    pub created_at: chrono::NaiveDateTime,
    #[serde(with = "chrono_datetime", default)]
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Environment {
    pub id: String,
    pub project_id: String,
    pub name: String,
    #[serde(rename = "base_url")]
    pub base_url: String,
    #[serde(default)]
    pub variables: HashMap<String, String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(rename = "tls_config", default)]
    pub tls_config: Option<TLSConfig>,
    #[serde(rename = "is_default", default)]
    pub is_default: bool,
    #[serde(with = "chrono_datetime", default)]
    pub created_at: chrono::NaiveDateTime,
    #[serde(with = "chrono_datetime", default)]
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TLSConfig {
    pub enabled: bool,
    #[serde(rename = "ca_file", default)]
    pub ca_file: Option<String>,
    #[serde(rename = "cert_file", default)]
    pub cert_file: Option<String>,
    #[serde(rename = "key_file", default)]
    pub key_file: Option<String>,
    #[serde(rename = "server_name", default)]
    pub server_name: Option<String>,
    #[serde(default)]
    pub insecure: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Collection {
    pub id: String,
    pub project_id: String,
    pub name: String,
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(default)]
    pub items: Vec<RequestItem>,
    #[serde(with = "chrono_datetime", default)]
    pub created_at: chrono::NaiveDateTime,
    #[serde(with = "chrono_datetime", default)]
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Folder {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub items: Vec<RequestItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestItem {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub service: String,
    pub method: String,
    pub body: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    #[serde(rename = "env_ref_type", default)]
    pub env_ref_type: Option<String>,
    #[serde(rename = "environment_id", default)]
    pub environment_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct History {
    pub id: String,
    #[serde(rename = "project_id", default)]
    pub project_id: Option<String>,
    pub timestamp: i64,
    pub service: String,
    pub method: String,
    pub address: String,
    pub status: String,
    #[serde(rename = "response_code", default)]
    pub response_code: Option<i32>,
    #[serde(rename = "response_message", default)]
    pub response_message: Option<String>,
    pub duration: i64,
    #[serde(rename = "request_snapshot")]
    pub request_snapshot: RequestItem,
    #[serde(with = "chrono_datetime", default)]
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub id: String,
    #[serde(rename = "project_id", default)]
    pub project_id: Option<String>,
    pub timestamp: i64,
    pub service: String,
    pub method: String,
    pub address: String,
    pub status: String,
    #[serde(rename = "response_code", default)]
    pub response_code: Option<i32>,
    #[serde(rename = "response_message", default)]
    pub response_message: Option<String>,
    pub duration: i64,
    #[serde(rename = "request_snapshot")]
    pub request_snapshot: RequestItem,
}

#[derive(Debug, Default)]
pub struct Filters {
    pub service: Option<String>,
    pub method: Option<String>,
    pub status: Option<String>,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// Create/Update structs
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateProject {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub proto_files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub description: Option<String>,
    pub proto_files: Option<Vec<String>>,
    #[serde(rename = "default_environment_id")]
    pub default_environment_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateEnvironment {
    pub project_id: String,
    pub name: String,
    #[serde(rename = "base_url")]
    pub base_url: String,
    #[serde(default)]
    pub variables: HashMap<String, String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(rename = "tls_config")]
    pub tls_config: Option<TLSConfig>,
    #[serde(rename = "is_default", default)]
    pub is_default: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UpdateEnvironment {
    pub name: Option<String>,
    #[serde(rename = "base_url")]
    pub base_url: Option<String>,
    pub variables: Option<HashMap<String, String>>,
    pub headers: Option<HashMap<String, String>>,
    #[serde(rename = "tls_config")]
    pub tls_config: Option<Option<TLSConfig>>,
    #[serde(rename = "is_default")]
    pub is_default: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateCollection {
    pub project_id: String,
    pub name: String,
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(default)]
    pub items: Vec<RequestItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UpdateCollection {
    pub name: Option<String>,
    pub folders: Option<Vec<Folder>>,
    pub items: Option<Vec<RequestItem>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateHistory {
    #[serde(rename = "project_id")]
    pub project_id: Option<String>,
    pub timestamp: i64,
    pub service: String,
    pub method: String,
    pub address: String,
    pub status: String,
    #[serde(rename = "response_code", default)]
    pub response_code: Option<i32>,
    #[serde(rename = "response_message", default)]
    pub response_message: Option<String>,
    pub duration: i64,
    #[serde(rename = "request_snapshot")]
    pub request_snapshot: RequestItem,
}

// Chrono datetime serialization module
mod chrono_datetime {
    use chrono::NaiveDateTime;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(dt: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(dt.and_utc().timestamp_millis())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = i64::deserialize(deserializer)?;
        Ok(chrono::DateTime::from_timestamp_millis(millis)
            .map(|dt| dt.naive_utc())
            .unwrap_or_else(|| chrono::Local::now().naive_local()))
    }
}

// Generate a new random storage ID (32-char hex)
pub fn new_storage_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
