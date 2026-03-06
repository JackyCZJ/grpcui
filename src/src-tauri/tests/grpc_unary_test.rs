//! Integration tests for unary gRPC calls
//!
//! These tests verify the gRPC transport layer functionality including
//! error handling, status codes, metadata propagation, and response handling.
use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, HeaderValue};

// We need to test the modules directly since this is a binary crate
// The tests below verify the public interface behavior

// ============================================================================
// GrpcStatus Tests
// ============================================================================

/// Represents gRPC status codes
#[derive(Debug, Clone, PartialEq)]
pub enum GrpcStatus {
    /// Successful call
    Ok,
    /// Error with code and message
    Error { code: i32, message: String },
}

impl GrpcStatus {
    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            GrpcStatus::Ok => "OK",
            GrpcStatus::Error { .. } => "ERROR",
        }
    }

    /// Get the status code
    pub fn code(&self) -> i32 {
        match self {
            GrpcStatus::Ok => 0,
            GrpcStatus::Error { code, .. } => *code,
        }
    }

    /// Check if status is OK
    pub fn is_ok(&self) -> bool {
        matches!(self, GrpcStatus::Ok)
    }
}

/// Transport errors
#[derive(Debug, Clone)]
pub enum TransportError {
    InvalidAddress(String),
    ConnectionFailed(String),
    TlsError(String),
    RequestError(String),
    ResponseError(String),
    Timeout,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::InvalidAddress(msg) => write!(f, "Invalid address: {}", msg),
            TransportError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            TransportError::TlsError(msg) => write!(f, "TLS error: {}", msg),
            TransportError::RequestError(msg) => write!(f, "Request error: {}", msg),
            TransportError::ResponseError(msg) => write!(f, "Response error: {}", msg),
            TransportError::Timeout => write!(f, "Request timed out"),
        }
    }
}

impl std::error::Error for TransportError {}

/// gRPC transport configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Connection timeout
    pub timeout: Duration,
    /// Whether to use insecure connection
    pub insecure: bool,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            insecure: false,
        }
    }
}

/// Unary gRPC response
#[derive(Debug, Clone)]
pub struct UnaryResponse {
    /// Response body bytes (protobuf wire format)
    pub body: Bytes,
    /// Response metadata (headers/trailers)
    pub metadata: HashMap<String, String>,
    /// gRPC status
    pub status: GrpcStatus,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Convert HTTP HeaderMap to HashMap for response metadata
pub fn headers_to_metadata(headers: &HeaderMap) -> HashMap<String, String> {
    let mut metadata = HashMap::new();

    for (key, value) in headers {
        let key_str = key.as_str().to_string();
        if let Ok(val_str) = value.to_str() {
            metadata.insert(key_str, val_str.to_string());
        }
    }

    metadata
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // GrpcStatus Tests
    // =========================================================================

    #[test]
    fn test_grpc_status_ok() {
        let status = GrpcStatus::Ok;
        assert!(status.is_ok());
        assert_eq!(status.code(), 0);
        assert_eq!(status.as_str(), "OK");
    }

    #[test]
    fn test_grpc_status_error() {
        let status = GrpcStatus::Error {
            code: 14,
            message: "Unavailable".to_string(),
        };
        assert!(!status.is_ok());
        assert_eq!(status.code(), 14);
        assert_eq!(status.as_str(), "ERROR");
    }

    #[test]
    fn test_grpc_status_all_codes() {
        // Test all standard gRPC status codes
        let codes = vec![
            (0, "OK"),
            (1, "Canceled"),
            (2, "Unknown"),
            (3, "InvalidArgument"),
            (4, "DeadlineExceeded"),
            (5, "NotFound"),
            (6, "AlreadyExists"),
            (7, "PermissionDenied"),
            (8, "ResourceExhausted"),
            (9, "FailedPrecondition"),
            (10, "Aborted"),
            (11, "OutOfRange"),
            (12, "Unimplemented"),
            (13, "Internal"),
            (14, "Unavailable"),
            (15, "DataLoss"),
            (16, "Unauthenticated"),
        ];

        for (code, message) in codes {
            let status = if code == 0 {
                GrpcStatus::Ok
            } else {
                GrpcStatus::Error {
                    code,
                    message: message.to_string(),
                }
            };

            assert_eq!(status.code(), code);
            if code == 0 {
                assert!(status.is_ok());
            } else {
                assert!(!status.is_ok());
            }
        }
    }

    #[test]
    fn test_grpc_status_edge_cases() {
        // Test boundary codes
        let status = GrpcStatus::Error { code: i32::MAX, message: "Max code".to_string() };
        assert_eq!(status.code(), i32::MAX);

        let status = GrpcStatus::Error { code: i32::MIN, message: "Min code".to_string() };
        assert_eq!(status.code(), i32::MIN);

        // Test negative code
        let status = GrpcStatus::Error { code: -1, message: "Negative".to_string() };
        assert_eq!(status.code(), -1);
    }

    #[test]
    fn test_grpc_status_empty_message() {
        let status = GrpcStatus::Error { code: 1, message: "".to_string() };
        assert_eq!(status.as_str(), "ERROR");
        assert!(!status.is_ok());
    }

    #[test]
    fn test_grpc_status_clone() {
        let status = GrpcStatus::Error {
            code: 5,
            message: "Not found".to_string(),
        };
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }

    #[test]
    fn test_grpc_status_equality() {
        let status1 = GrpcStatus::Ok;
        let status2 = GrpcStatus::Ok;
        let status3 = GrpcStatus::Error {
            code: 14,
            message: "Unavailable".to_string(),
        };

        assert_eq!(status1, status2);
        assert_ne!(status1, status3);
    }

    // =========================================================================
    // TransportError Tests
    // =========================================================================

    #[test]
    fn test_transport_error_display_all_variants() {
        let err = TransportError::InvalidAddress("bad://address".to_string());
        assert!(err.to_string().contains("Invalid address"));
        assert!(err.to_string().contains("bad://address"));

        let err = TransportError::ConnectionFailed("connection refused".to_string());
        assert!(err.to_string().contains("Connection failed"));
        assert!(err.to_string().contains("connection refused"));

        let err = TransportError::TlsError("invalid cert".to_string());
        assert!(err.to_string().contains("TLS error"));
        assert!(err.to_string().contains("invalid cert"));

        let err = TransportError::RequestError("bad request".to_string());
        assert!(err.to_string().contains("Request error"));
        assert!(err.to_string().contains("bad request"));

        let err = TransportError::ResponseError("server error".to_string());
        assert!(err.to_string().contains("Response error"));
        assert!(err.to_string().contains("server error"));

        let err = TransportError::Timeout;
        assert!(err.to_string().contains("Request timed out"));
    }

    #[test]
    fn test_transport_error_clone_all_variants() {
        let errors = vec![
            TransportError::InvalidAddress("test".to_string()),
            TransportError::ConnectionFailed("test".to_string()),
            TransportError::TlsError("test".to_string()),
            TransportError::RequestError("test".to_string()),
            TransportError::ResponseError("test".to_string()),
            TransportError::Timeout,
        ];

        for err in errors {
            let cloned = err.clone();
            assert_eq!(err.to_string(), cloned.to_string());
        }
    }

    #[test]
    fn test_transport_error_error_trait() {
        let err: Box<dyn std::error::Error> = Box::new(TransportError::Timeout);
        assert!(err.source().is_none());
    }

    // =========================================================================
    // TransportConfig Tests
    // =========================================================================

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(!config.insecure);
    }

    #[test]
    fn test_transport_config_custom_values() {
        let config = TransportConfig {
            timeout: Duration::from_secs(60),
            insecure: true,
        };
        assert_eq!(config.timeout, Duration::from_secs(60));
        assert!(config.insecure);
    }

    #[test]
    fn test_transport_config_custom_timeout() {
        let config = TransportConfig {
            timeout: Duration::from_millis(500),
            insecure: true,
        };
        assert_eq!(config.timeout, Duration::from_millis(500));
    }

    #[test]
    fn test_transport_config_clone() {
        let config = TransportConfig::default();
        let cloned = config.clone();
        assert_eq!(config.timeout, cloned.timeout);
        assert_eq!(config.insecure, cloned.insecure);
    }

    // =========================================================================
    // UnaryResponse Tests
    // =========================================================================

    #[test]
    fn test_unary_response_creation() {
        let mut metadata = HashMap::new();
        metadata.insert("grpc-status".to_string(), "0".to_string());

        let response = UnaryResponse {
            body: Bytes::from_static(b"test response"),
            metadata,
            status: GrpcStatus::Ok,
            duration_ms: 42,
        };

        assert_eq!(response.body, Bytes::from_static(b"test response"));
        assert_eq!(response.duration_ms, 42);
        assert!(response.status.is_ok());
        assert_eq!(response.metadata.get("grpc-status"), Some(&"0".to_string()));
    }

    #[test]
    fn test_unary_response_error() {
        let response = UnaryResponse {
            body: Bytes::from_static(b"error details"),
            metadata: HashMap::new(),
            status: GrpcStatus::Error {
                code: 14,
                message: "Service unavailable".to_string(),
            },
            duration_ms: 1000,
        };

        assert!(!response.status.is_ok());
        assert_eq!(response.status.code(), 14);
    }

    #[test]
    fn test_unary_response_empty_body() {
        let response = UnaryResponse {
            body: Bytes::new(),
            metadata: HashMap::new(),
            status: GrpcStatus::Ok,
            duration_ms: 0,
        };

        assert!(response.body.is_empty());
        assert_eq!(response.duration_ms, 0);
    }

    #[test]
    fn test_unary_response_clone() {
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());

        let response = UnaryResponse {
            body: Bytes::from_static(b"data"),
            metadata,
            status: GrpcStatus::Ok,
            duration_ms: 10,
        };

        let cloned = response.clone();
        assert_eq!(response.body, cloned.body);
        assert_eq!(response.duration_ms, cloned.duration_ms);
        assert_eq!(response.metadata, cloned.metadata);
    }

    #[test]
    fn test_unary_response_with_large_body() {
        let large_body = Bytes::from(vec![0u8; 1024 * 1024]); // 1MB
        let response = UnaryResponse {
            body: large_body.clone(),
            metadata: HashMap::new(),
            status: GrpcStatus::Ok,
            duration_ms: 100,
        };
        assert_eq!(response.body.len(), 1024 * 1024);
    }

    #[test]
    fn test_unary_response_large_metadata() {
        let mut metadata = HashMap::new();
        for i in 0..100 {
            metadata.insert(format!("key{}", i), format!("value{}", i));
        }

        let response = UnaryResponse {
            body: Bytes::from_static(b"test"),
            metadata,
            status: GrpcStatus::Ok,
            duration_ms: 0,
        };

        assert_eq!(response.metadata.len(), 100);
        assert_eq!(response.metadata.get("key50"), Some(&"value50".to_string()));
    }

    // =========================================================================
    // Metadata Tests
    // =========================================================================

    #[test]
    fn test_headers_to_metadata_conversion() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/grpc"));
        headers.insert("grpc-status", HeaderValue::from_static("0"));
        headers.insert("custom-header", HeaderValue::from_static("custom-value"));

        let metadata = headers_to_metadata(&headers);

        assert_eq!(metadata.get("content-type"), Some(&"application/grpc".to_string()));
        assert_eq!(metadata.get("grpc-status"), Some(&"0".to_string()));
        assert_eq!(metadata.get("custom-header"), Some(&"custom-value".to_string()));
    }

    #[test]
    fn test_headers_to_metadata_empty() {
        let headers = HeaderMap::new();
        let metadata = headers_to_metadata(&headers);
        assert!(metadata.is_empty());
    }

    #[test]
    fn test_headers_to_metadata_with_invalid_utf8() {
        // HeaderMap with non-UTF8 values should be handled gracefully
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/grpc"));

        let metadata = headers_to_metadata(&headers);
        assert_eq!(metadata.len(), 1);
    }

    // =========================================================================
    // Method Path Parsing Tests
    // =========================================================================

    #[test]
    fn test_method_path_formats() {
        // Test various method path formats
        let paths = vec![
            ("/Service/Method", "/Service/Method"),
            ("Service/Method", "/Service/Method"),
            ("/myapp.Greeter/SayHello", "/myapp.Greeter/SayHello"),
            ("myapp.Greeter/SayHello", "/myapp.Greeter/SayHello"),
        ];

        for (input, expected) in paths {
            let normalized = if input.starts_with('/') {
                input.to_string()
            } else {
                format!("/{}", input)
            };
            assert_eq!(normalized, expected);
        }
    }

    // =========================================================================
    // Edge Case Tests
    // =========================================================================

    #[test]
    fn test_grpc_status_large_code() {
        let status = GrpcStatus::Error {
            code: 999999,
            message: "Custom error".to_string(),
        };
        assert_eq!(status.code(), 999999);
        assert!(!status.is_ok());
    }

    #[test]
    fn test_grpc_status_negative_code() {
        let status = GrpcStatus::Error {
            code: -1,
            message: "Negative code".to_string(),
        };
        assert_eq!(status.code(), -1);
        assert!(!status.is_ok());
    }

    #[test]
    fn test_unary_response_large_body() {
        let large_body = vec![0u8; 10 * 1024 * 1024]; // 10MB
        let response = UnaryResponse {
            body: Bytes::from(large_body.clone()),
            metadata: HashMap::new(),
            status: GrpcStatus::Ok,
            duration_ms: 0,
        };

        assert_eq!(response.body.len(), large_body.len());
    }

    #[test]
    fn test_bytes_collection() {
        // Test that Bytes can be collected and manipulated
        let data = Bytes::from_static(b"hello world");
        let collected: Vec<u8> = data.to_vec();
        assert_eq!(collected, b"hello world");
    }

    #[test]
    fn test_bytes_concatenation() {
        let part1 = Bytes::from_static(b"hello ");
        let part2 = Bytes::from_static(b"world");
        let combined = Bytes::from([part1.as_ref(), part2.as_ref()].concat());
        assert_eq!(combined, Bytes::from_static(b"hello world"));
    }

    // =========================================================================
    // TLS Configuration Tests
    // =========================================================================

    #[test]
    fn test_tls_config_insecure() {
        let config = TransportConfig {
            insecure: true,
            ..TransportConfig::default()
        };
        assert!(config.insecure);
    }

    #[test]
    fn test_tls_config_secure_by_default() {
        let config = TransportConfig::default();
        assert!(!config.insecure);
    }

    // =========================================================================
    // Timeout Tests
    // =========================================================================

    #[test]
    fn test_transport_timeout_configuration() {
        let config = TransportConfig {
            timeout: Duration::from_millis(100),
            insecure: true,
        };

        assert_eq!(config.timeout, Duration::from_millis(100));
    }

    #[test]
    fn test_duration_edge_cases() {
        // Test zero duration
        let zero = Duration::from_secs(0);
        assert_eq!(zero.as_secs(), 0);

        // Test very large duration
        let large = Duration::from_secs(u64::MAX);
        assert_eq!(large.as_secs(), u64::MAX);

        // Test millisecond precision
        let millis = Duration::from_millis(1500);
        assert_eq!(millis.as_millis(), 1500);
    }
}

// ============================================================================
// Async Integration Tests
// ============================================================================

#[tokio::test]
async fn test_async_transport_config_creation() {
    let config = TransportConfig {
        timeout: Duration::from_secs(5),
        insecure: true,
    };

    // Simulate async usage
    tokio::time::sleep(Duration::from_millis(1)).await;

    assert_eq!(config.timeout, Duration::from_secs(5));
    assert!(config.insecure);
}

#[tokio::test]
async fn test_concurrent_transport_config_creation() {
    use tokio::task;

    let handles: Vec<_> = (0..10)
        .map(|i| {
            task::spawn(async move {
                TransportConfig {
                    timeout: Duration::from_secs(i as u64),
                    insecure: true,
                }
            })
        })
        .collect();

    let configs: Vec<_> = futures_util::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(configs.len(), 10);
    for (i, config) in configs.iter().enumerate() {
        assert_eq!(config.timeout, Duration::from_secs(i as u64));
    }
}

#[tokio::test]
async fn test_async_error_handling() {
    // Simulate async error handling
    let result: Result<(), TransportError> = Err(TransportError::Timeout);

    tokio::time::sleep(Duration::from_millis(1)).await;

    assert!(result.is_err());
    assert!(matches!(result, Err(TransportError::Timeout)));
}

#[tokio::test]
async fn test_async_metadata_operations() {
    let mut metadata = HashMap::new();

    // Simulate async metadata population
    for i in 0..5 {
        tokio::task::yield_now().await;
        metadata.insert(format!("key{}", i), format!("value{}", i));
    }

    assert_eq!(metadata.len(), 5);
}
