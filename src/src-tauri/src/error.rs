use std::fmt;
use serde::{Deserialize, Serialize};

/// Application error types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppError {
    /// Sidecar-related errors
    SidecarNotRunning,
    SidecarStartFailed(String),
    SidecarCommunicationFailed(String),

    /// gRPC-related errors
    GrpcConnectionFailed(String),
    GrpcInvokeFailed(String),
    GrpcStreamFailed(String),
    GrpcServiceNotFound(String),
    GrpcMethodNotFound(String),

    /// Storage-related errors
    StorageReadFailed(String),
    StorageWriteFailed(String),
    StorageNotFound(String),

    /// Serialization errors
    SerializationFailed(String),
    DeserializationFailed(String),

    /// Network errors
    NetworkError(String),
    TimeoutError,

    /// Unknown errors
    Unknown(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::SidecarNotRunning => write!(f, "Sidecar is not running"),
            AppError::SidecarStartFailed(msg) => write!(f, "Failed to start sidecar: {}", msg),
            AppError::SidecarCommunicationFailed(msg) => write!(f, "Sidecar communication failed: {}", msg),
            AppError::GrpcConnectionFailed(msg) => write!(f, "gRPC connection failed: {}", msg),
            AppError::GrpcInvokeFailed(msg) => write!(f, "gRPC invocation failed: {}", msg),
            AppError::GrpcStreamFailed(msg) => write!(f, "gRPC stream failed: {}", msg),
            AppError::GrpcServiceNotFound(name) => write!(f, "Service not found: {}", name),
            AppError::GrpcMethodNotFound(name) => write!(f, "Method not found: {}", name),
            AppError::StorageReadFailed(msg) => write!(f, "Storage read failed: {}", msg),
            AppError::StorageWriteFailed(msg) => write!(f, "Storage write failed: {}", msg),
            AppError::StorageNotFound(key) => write!(f, "Storage item not found: {}", key),
            AppError::SerializationFailed(msg) => write!(f, "Serialization failed: {}", msg),
            AppError::DeserializationFailed(msg) => write!(f, "Deserialization failed: {}", msg),
            AppError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            AppError::TimeoutError => write!(f, "Operation timed out"),
            AppError::Unknown(msg) => write!(f, "Unknown error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

/// Convert from reqwest errors
impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            AppError::TimeoutError
        } else if err.is_connect() {
            AppError::NetworkError(err.to_string())
        } else {
            AppError::SidecarCommunicationFailed(err.to_string())
        }
    }
}

/// Convert from serde_json errors
impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::DeserializationFailed(err.to_string())
    }
}

/// Convert from std::io errors
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::SidecarStartFailed(err.to_string())
    }
}

/// Result type alias using AppError
pub type Result<T> = std::result::Result<T, AppError>;

/// Convert Result to a Tauri-compatible Result
///
/// 目前命令层直接返回 `Result<T, String>`，该 trait 保留给后续命令统一错误映射时使用。
#[allow(dead_code)]
pub trait IntoTauriResult<T> {
    fn into_tauri(self) -> std::result::Result<T, String>;
}

impl<T> IntoTauriResult<T> for Result<T> {
    fn into_tauri(self) -> std::result::Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}

/// Error response for frontend
///
/// 当前前端暂未消费结构化错误码，先保留类型定义，后续启用统一错误响应时直接复用。
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

impl From<AppError> for ErrorResponse {
    fn from(err: AppError) -> Self {
        let code = match &err {
            AppError::SidecarNotRunning => "SIDECAR_NOT_RUNNING",
            AppError::SidecarStartFailed(_) => "SIDECAR_START_FAILED",
            AppError::SidecarCommunicationFailed(_) => "SIDECAR_COMMUNICATION_FAILED",
            AppError::GrpcConnectionFailed(_) => "GRPC_CONNECTION_FAILED",
            AppError::GrpcInvokeFailed(_) => "GRPC_INVOKE_FAILED",
            AppError::GrpcStreamFailed(_) => "GRPC_STREAM_FAILED",
            AppError::GrpcServiceNotFound(_) => "GRPC_SERVICE_NOT_FOUND",
            AppError::GrpcMethodNotFound(_) => "GRPC_METHOD_NOT_FOUND",
            AppError::StorageReadFailed(_) => "STORAGE_READ_FAILED",
            AppError::StorageWriteFailed(_) => "STORAGE_WRITE_FAILED",
            AppError::StorageNotFound(_) => "STORAGE_NOT_FOUND",
            AppError::SerializationFailed(_) => "SERIALIZATION_FAILED",
            AppError::DeserializationFailed(_) => "DESERIALIZATION_FAILED",
            AppError::NetworkError(_) => "NETWORK_ERROR",
            AppError::TimeoutError => "TIMEOUT_ERROR",
            AppError::Unknown(_) => "UNKNOWN_ERROR",
        };

        ErrorResponse {
            code: code.to_string(),
            message: err.to_string(),
            details: None,
        }
    }
}

/// Helper function to convert Go error strings to AppError
///
/// 当前命令返回原始字符串错误，此函数保留用于后续统一错误语义映射。
#[allow(dead_code)]
pub fn parse_go_error(error_str: &str) -> AppError {
    if error_str.contains("connection refused") {
        AppError::GrpcConnectionFailed(error_str.to_string())
    } else if error_str.contains("timeout") {
        AppError::TimeoutError
    } else if error_str.contains("not found") || error_str.contains("unknown service") {
        AppError::GrpcServiceNotFound(error_str.to_string())
    } else if error_str.contains("unknown method") {
        AppError::GrpcMethodNotFound(error_str.to_string())
    } else {
        AppError::GrpcInvokeFailed(error_str.to_string())
    }
}
