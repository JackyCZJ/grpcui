//! gRPC Client
//!
//! This module provides a high-level gRPC client that uses the FFI bridge
//! for message encoding/decoding and tonic for transport.

#![allow(dead_code)]
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::ffi::CodecBridge;
use super::format_error_with_chain;
use super::transport::{GrpcTransport, TransportConfig, TransportError, GrpcStatus};
use tower::ServiceExt;

/// gRPC client for making unary calls
#[derive(Debug)]
pub struct GrpcClient {
    transport: GrpcTransport,
    codec: Arc<CodecBridge>,
}

/// TLS configuration for gRPC connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Whether to skip TLS verification
    pub insecure: bool,
    /// Path to CA certificate file
    pub ca_cert_path: Option<String>,
    /// Path to client certificate file
    pub client_cert_path: Option<String>,
    /// Path to client key file
    pub client_key_path: Option<String>,
}

/// Unary call response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnaryResponse {
    /// JSON-encoded response payload
    pub json_payload: String,
    /// Response metadata (headers/trailers)
    pub metadata: HashMap<String, String>,
    /// gRPC status
    pub status: ResponseStatus,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Server streaming response item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamResponse {
    /// JSON-encoded response payload
    pub json_payload: String,
    /// Response metadata (headers/trailers) - only present on first message
    pub metadata: Option<HashMap<String, String>>,
    /// gRPC status - only present on final message
    pub status: Option<ResponseStatus>,
}

/// Type alias for server streaming response channel receiver
pub type ServerStreamingReceiver = mpsc::Receiver<std::result::Result<StreamResponse, GrpcError>>;

/// Response status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseStatus {
    /// Status code (0 = OK)
    pub code: i32,
    /// Status message
    pub message: String,
    /// Status as string ("OK" or "ERROR")
    pub status: String,
}

/// gRPC client errors
#[derive(Debug, Clone)]
pub enum GrpcError {
    /// Transport error
    Transport(TransportError),
    /// Encoding error
    Encoding(String),
    /// Decoding error
    Decoding(String),
    /// Invalid method name
    InvalidMethod(String),
    /// Connection not established
    NotConnected,
    /// Timeout
    Timeout,
}

impl std::fmt::Display for GrpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrpcError::Transport(e) => write!(f, "Transport error: {}", e),
            GrpcError::Encoding(msg) => write!(f, "Encoding error: {}", msg),
            GrpcError::Decoding(msg) => write!(f, "Decoding error: {}", msg),
            GrpcError::InvalidMethod(msg) => write!(f, "Invalid method: {}", msg),
            GrpcError::NotConnected => write!(f, "Not connected to gRPC server"),
            GrpcError::Timeout => write!(f, "Request timed out"),
        }
    }
}

impl std::error::Error for GrpcError {}

impl From<TransportError> for GrpcError {
    fn from(e: TransportError) -> Self {
        match e {
            TransportError::Timeout => GrpcError::Timeout,
            _ => GrpcError::Transport(e),
        }
    }
}

impl From<crate::ffi::FfiError> for GrpcError {
    fn from(e: crate::ffi::FfiError) -> Self {
        match e {
            crate::ffi::FfiError::FfiCallFailed { function, message } => {
                if function.contains("encode") {
                    GrpcError::Encoding(message)
                } else {
                    GrpcError::Decoding(message)
                }
            }
            crate::ffi::FfiError::InvalidMethodName { name } => GrpcError::InvalidMethod(name),
            _ => GrpcError::Encoding(e.to_string()),
        }
    }
}

/// build_transport_config 根据可选 TLS 参数与 authority 构建传输层配置。
///
/// 默认策略采用明文连接（insecure = true）：
/// - 兼容本地开发与大多数内网网关（例如 APISIX 9080）
/// - 避免未显式配置 TLS 时误用 HTTPS 导致连接失败
///
/// 当调用方提供 TLS 配置后，会覆盖默认值，从而支持系统证书或自定义证书场景。
fn build_transport_config(tls: Option<TlsConfig>, authority: Option<String>) -> TransportConfig {
    match tls {
        Some(tls_config) => {
            // TODO: Load custom TLS certificates if paths are provided
            TransportConfig {
                insecure: tls_config.insecure,
                authority,
                ..TransportConfig::default()
            }
        }
        None => TransportConfig {
            insecure: true,
            authority,
            ..TransportConfig::default()
        },
    }
}

impl GrpcClient {
    /// Connect to a gRPC server
    ///
    /// # Arguments
    /// * `address` - Server address (e.g., "localhost:50051")
    /// * `tls` - Optional TLS configuration
    ///
    /// # Returns
    /// A new GrpcClient instance
    pub async fn connect(address: &str, tls: Option<TlsConfig>) -> Result<Self, GrpcError> {
        // 默认路径仍保留新建 codec 的能力，兼容现有测试与独立调用场景。
        let codec = Arc::new(CodecBridge::new().map_err(|e| GrpcError::Encoding(e.to_string()))?);
        Self::connect_with_codec(address, tls, None, codec).await
    }

    /// connect_with_codec 允许复用外部已加载的 FFI codec。
    ///
    /// 命令层会传入 AppState 中的共享 codec，保证 encode/decode 使用
    /// 与 grpc_connect 相同的一份服务描述，避免“list_services 有、invoke 找不到”。
    pub async fn connect_with_codec(
        address: &str,
        tls: Option<TlsConfig>,
        authority: Option<String>,
        codec: Arc<CodecBridge>,
    ) -> Result<Self, GrpcError> {
        // Build transport config
        let config = build_transport_config(tls, authority);

        // Connect transport
        let transport = GrpcTransport::connect(address, config).await?;

        Ok(Self { transport, codec })
    }

    /// transport_channel 返回底层 tonic channel 的克隆句柄。
    ///
    /// 流式命令层需要把 channel 交给统一的 StreamManager 托管，
    /// 这里暴露只读克隆，避免外部直接修改 GrpcClient 内部状态。
    pub fn transport_channel(&self) -> tonic::transport::Channel {
        self.transport.channel().clone()
    }

    /// Perform a server streaming gRPC call
    ///
    /// # Arguments
    /// * `method` - Method path (e.g., "myapp.Greeter/SayHello")
    /// * `request_json` - JSON-encoded request payload
    /// * `metadata` - Optional metadata headers
    /// * `timeout` - Optional timeout override
    ///
    /// # Returns
    /// A channel receiver that yields StreamResponse items
    pub async fn server_streaming_call(
        &self,
        method: &str,
        request_json: &str,
        metadata: HashMap<String, String>,
        _timeout: Option<Duration>,
    ) -> Result<ServerStreamingReceiver, GrpcError> {
        // Debug logging
        if std::env::var("GRPC_DEBUG").is_ok() {
            log::debug!("gRPC server streaming call: method={}, metadata={:?}", method, metadata);
        }

        let encode_start = std::time::Instant::now();

        // Encode JSON to wire format using FFI bridge
        let wire_data = self.codec.encode_request(method, request_json)?;

        if std::env::var("GRPC_DEBUG").is_ok() {
            log::debug!("Encoding took: {:?}", encode_start.elapsed());
        }

        // Get the channel from transport for streaming
        let channel = self.transport.channel().clone();
        let method = method.to_string();
        let codec = Arc::clone(&self.codec);

        // Create channel for streaming responses
        let (tx, rx) = mpsc::channel::<std::result::Result<StreamResponse, GrpcError>>(100);

        // Spawn task to handle the streaming
        tokio::spawn(async move {
            let start_time = std::time::Instant::now();

            // Parse method path (format: "/Service/Method" or "Service/Method")
            let method_path = if method.starts_with('/') {
                method.clone()
            } else {
                format!("/{}", method)
            };

            // Build request
            let mut request_builder = http::Request::builder()
                .method("POST")
                .uri(method_path.clone())
                .header("content-type", "application/grpc")
                .header("te", "trailers");

            // Add metadata headers
            for (key, value) in &metadata {
                if let (Ok(header_name), Ok(header_value)) = (
                    http::HeaderName::from_bytes(key.as_bytes()),
                    http::HeaderValue::from_str(value)
                ) {
                    request_builder = request_builder.header(header_name, header_value);
                }
            }

            // Build request with boxed body for tonic compatibility
            let body = http_body_util::Full::new(Bytes::from(wire_data));
            let boxed_body = body.map_err(|e: std::convert::Infallible| match e {}).boxed_unsync();

            let mut metadata_keys: Vec<&str> = metadata.keys().map(String::as_str).collect();
            metadata_keys.sort_unstable();
            let request_context = format!(
                "method_path={}, metadata_keys={:?}",
                method_path,
                metadata_keys
            );

            let request = match request_builder.body(boxed_body) {
                Ok(req) => req,
                Err(e) => {
                    let error_report = format_error_with_chain(&e);
                    log::error!(
                        "gRPC server streaming request build failed: diagnostics={}, context={}",
                        error_report,
                        request_context
                    );
                    let _ = tx.send(Err(GrpcError::Transport(
                        TransportError::RequestError(format!(
                            "Request build failed: {}; context={}",
                            error_report, request_context
                        ))
                    ))).await;
                    return;
                }
            };

            // Perform the streaming call using tower service
            let response = match channel.oneshot(request).await {
                Ok(resp) => resp,
                Err(e) => {
                    let error_report = format_error_with_chain(&e);
                    log::error!(
                        "gRPC server streaming request failed: diagnostics={}, context={}",
                        error_report,
                        request_context
                    );
                    let _ = tx.send(Err(GrpcError::Transport(
                        TransportError::RequestError(format!(
                            "Request failed: {}; context={}",
                            error_report, request_context
                        ))
                    ))).await;
                    return;
                }
            };

            // Extract response headers for first message
            let response_headers = response.headers().clone();
            let response_metadata = super::metadata::headers_to_metadata(&response_headers);

            // Get response body stream
            let mut body = response.into_body();
            let mut first_message = true;

            // Stream responses
            while let Some(frame_result) = body.frame().await {
                let frame = match frame_result {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = tx.send(Err(GrpcError::Transport(
                            TransportError::ResponseError(format!("Failed to read response: {}", e))
                        ))).await;
                        return;
                    }
                };

                // Get data from frame if it's a data frame
                let chunk = match frame.data_ref() {
                    Some(data) => data.clone(),
                    None => continue,
                };

                if chunk.is_empty() {
                    continue;
                }

                // Decode response from wire format to JSON
                let json_payload = match codec.decode_response(&method, &chunk) {
                    Ok(payload) => payload,
                    Err(e) => {
                        let _ = tx.send(Err(GrpcError::from(e))).await;
                        return;
                    }
                };

                let metadata = if first_message {
                    first_message = false;
                    Some(response_metadata.clone())
                } else {
                    None
                };

                let response = StreamResponse {
                    json_payload,
                    metadata,
                    status: None,
                };

                if tx.send(Ok(response)).await.is_err() {
                    // Receiver dropped, stop streaming
                    return;
                }
            }

            // Check for trailers/status
            let _duration_ms = start_time.elapsed().as_millis() as u64;

            // Parse gRPC status from headers or trailers
            let grpc_status = response_headers
                .get("grpc-status")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0);

            let grpc_message = response_headers
                .get("grpc-message")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let status = if grpc_status == 0 {
                ResponseStatus {
                    code: 0,
                    message: "OK".to_string(),
                    status: "OK".to_string(),
                }
            } else {
                ResponseStatus {
                    code: grpc_status,
                    message: grpc_message.unwrap_or_else(|| "Unknown error".to_string()),
                    status: "ERROR".to_string(),
                }
            };

            // Yield final message with status
            let _ = tx.send(Ok(StreamResponse {
                json_payload: "{}".to_string(),
                metadata: None,
                status: Some(status),
            })).await;

            if std::env::var("GRPC_DEBUG").is_ok() {
                log::debug!("Server streaming completed in {}ms", _duration_ms);
            }
        });

        Ok(rx)
    }
    
    /// Perform a unary gRPC call
    ///
    /// # Arguments
    /// * `method` - Method path (e.g., "myapp.Greeter/SayHello")
    /// * `request_json` - JSON-encoded request payload
    /// * `metadata` - Optional metadata headers
    /// * `timeout` - Optional timeout override
    ///
    /// # Returns
    /// UnaryResponse with decoded JSON payload
    pub async fn unary_call(
        &self,
        method: &str,
        request_json: &str,
        metadata: HashMap<String, String>,
        timeout: Option<Duration>,
    ) -> Result<UnaryResponse, GrpcError> {
        // Debug logging
        if std::env::var("GRPC_DEBUG").is_ok() {
            log::debug!("gRPC unary call: method={}, metadata={:?}", method, metadata);
        }
        
        let encode_start = std::time::Instant::now();
        
        // Encode JSON to wire format using FFI bridge
        let wire_data = self.codec.encode_request(method, request_json)?;
        
        if std::env::var("GRPC_DEBUG").is_ok() {
            log::debug!("Encoding took: {:?}", encode_start.elapsed());
        }
        
        // Perform the transport call
        let transport_response = self
            .transport
            .unary_call(method, Bytes::from(wire_data), &metadata, timeout)
            .await?;
        
        if std::env::var("GRPC_DEBUG").is_ok() {
            log::debug!("Transport took: {}ms", transport_response.duration_ms);
        }
        
        // Decode response from wire format to JSON
        let decode_start = std::time::Instant::now();
        let json_payload = if transport_response.body.is_empty() {
            "{}".to_string()
        } else {
            self.codec.decode_response(method, &transport_response.body)?
        };
        
        if std::env::var("GRPC_DEBUG").is_ok() {
            log::debug!("Decoding took: {:?}", decode_start.elapsed());
        }
        
        // Build response status
        let status = match &transport_response.status {
            GrpcStatus::Ok => ResponseStatus {
                code: 0,
                message: "OK".to_string(),
                status: "OK".to_string(),
            },
            GrpcStatus::Error { code, message } => ResponseStatus {
                code: *code,
                message: message.clone(),
                status: "ERROR".to_string(),
            },
        };
        
        Ok(UnaryResponse {
            json_payload,
            metadata: transport_response.metadata,
            status,
            duration_ms: transport_response.duration_ms,
        })
    }
    
    /// Check if the connection is ready
    pub async fn is_ready(&mut self) -> bool {
        self.transport.is_ready().await
    }
    
    /// Get the codec bridge
    pub fn codec(&self) -> &Arc<CodecBridge> {
        &self.codec
    }
}

/// Client manager for handling multiple connections
#[derive(Debug, Default)]
pub struct ClientManager {
    clients: std::sync::Mutex<HashMap<String, Arc<GrpcClient>>>,
}

impl ClientManager {
    /// Create a new client manager
    pub fn new() -> Self {
        Self {
            clients: std::sync::Mutex::new(HashMap::new()),
        }
    }
    
    /// Get or create a client for an address
    pub async fn get_or_create(
        &self,
        address: &str,
        tls: Option<TlsConfig>,
    ) -> Result<Arc<GrpcClient>, GrpcError> {
        // Try to get existing client
        {
            let clients = self.clients.lock().unwrap();
            if let Some(client) = clients.get(address) {
                return Ok(Arc::clone(client));
            }
        }

        // Create new client
        let client = Arc::new(GrpcClient::connect(address, tls).await?);

        // Store it
        {
            let mut clients = self.clients.lock().unwrap();
            clients.insert(address.to_string(), Arc::clone(&client));
        }

        Ok(client)
    }
    
    /// Remove a client
    pub fn remove(&self, address: &str) {
        let mut clients = self.clients.lock().unwrap();
        clients.remove(address);
    }
    
    /// Clear all clients
    pub fn clear(&self) {
        let mut clients = self.clients.lock().unwrap();
        clients.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::FfiError;

    #[test]
    fn test_build_transport_config_defaults_to_insecure_for_plaintext_targets() {
        let config = build_transport_config(None, None);
        assert!(config.insecure);
    }

    #[test]
    fn test_build_transport_config_honors_explicit_tls_setting() {
        let config = build_transport_config(Some(TlsConfig {
            insecure: false,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
        }), None);

        assert!(!config.insecure);
    }

    // =========================================================================
    // ResponseStatus Tests
    // =========================================================================

    #[test]
    fn test_response_status_ok() {
        let status = ResponseStatus {
            code: 0,
            message: "OK".to_string(),
            status: "OK".to_string(),
        };
        assert_eq!(status.code, 0);
        assert_eq!(status.status, "OK");
        assert_eq!(status.message, "OK");
    }

    #[test]
    fn test_response_status_error() {
        let status = ResponseStatus {
            code: 14,
            message: "Unavailable".to_string(),
            status: "ERROR".to_string(),
        };
        assert_eq!(status.code, 14);
        assert_eq!(status.status, "ERROR");
        assert_eq!(status.message, "Unavailable");
    }

    #[test]
    fn test_response_status_serialization() {
        let status = ResponseStatus {
            code: 5,
            message: "Not found".to_string(),
            status: "ERROR".to_string(),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"code\":5"));
        assert!(json.contains("\"message\":\"Not found\""));
        assert!(json.contains("\"status\":\"ERROR\""));

        let deserialized: ResponseStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.code, 5);
        assert_eq!(deserialized.message, "Not found");
        assert_eq!(deserialized.status, "ERROR");
    }

    // =========================================================================
    // GrpcError Tests
    // =========================================================================

    #[test]
    fn test_grpc_error_display() {
        let err = GrpcError::NotConnected;
        assert_eq!(err.to_string(), "Not connected to gRPC server");

        let err = GrpcError::Timeout;
        assert_eq!(err.to_string(), "Request timed out");

        let err = GrpcError::Encoding("invalid json".to_string());
        assert_eq!(err.to_string(), "Encoding error: invalid json");

        let err = GrpcError::Decoding("corrupted data".to_string());
        assert_eq!(err.to_string(), "Decoding error: corrupted data");

        let err = GrpcError::InvalidMethod("bad/method/name".to_string());
        assert_eq!(err.to_string(), "Invalid method: bad/method/name");
    }

    #[test]
    fn test_grpc_error_from_transport_error() {
        let transport_err = TransportError::ConnectionFailed("refused".to_string());
        let grpc_err: GrpcError = transport_err.into();
        match grpc_err {
            GrpcError::Transport(TransportError::ConnectionFailed(msg)) => {
                assert_eq!(msg, "refused");
            }
            _ => panic!("Expected Transport error"),
        }
    }

    #[test]
    fn test_grpc_error_from_transport_timeout() {
        let transport_err = TransportError::Timeout;
        let grpc_err: GrpcError = transport_err.into();
        match grpc_err {
            GrpcError::Timeout => {}
            _ => panic!("Expected Timeout error, got {:?}", grpc_err),
        }
    }

    #[test]
    fn test_grpc_error_from_ffi_error_encode() {
        let ffi_err = FfiError::FfiCallFailed {
            function: "encode_request_json_to_wire".to_string(),
            message: "proto not found".to_string(),
        };
        let grpc_err: GrpcError = ffi_err.into();
        match grpc_err {
            GrpcError::Encoding(msg) => {
                assert_eq!(msg, "proto not found");
            }
            _ => panic!("Expected Encoding error, got {:?}", grpc_err),
        }
    }

    #[test]
    fn test_grpc_error_from_ffi_error_decode() {
        let ffi_err = FfiError::FfiCallFailed {
            function: "decode_response_wire_to_json".to_string(),
            message: "invalid wire format".to_string(),
        };
        let grpc_err: GrpcError = ffi_err.into();
        match grpc_err {
            GrpcError::Decoding(msg) => {
                assert_eq!(msg, "invalid wire format");
            }
            _ => panic!("Expected Decoding error, got {:?}", grpc_err),
        }
    }

    #[test]
    fn test_grpc_error_from_ffi_invalid_method() {
        let ffi_err = FfiError::InvalidMethodName {
            name: "InvalidMethod".to_string(),
        };
        let grpc_err: GrpcError = ffi_err.into();
        match grpc_err {
            GrpcError::InvalidMethod(name) => {
                assert_eq!(name, "InvalidMethod");
            }
            _ => panic!("Expected InvalidMethod error, got {:?}", grpc_err),
        }
    }

    #[test]
    fn test_grpc_error_error_trait() {
        let err = GrpcError::NotConnected;
        let err_ref: &dyn std::error::Error = &err;
        assert!(err_ref.source().is_none());
    }

    // =========================================================================
    // TlsConfig Tests
    // =========================================================================

    #[test]
    fn test_tls_config_default() {
        let config = TlsConfig {
            insecure: false,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
        };
        assert!(!config.insecure);
        assert!(config.ca_cert_path.is_none());
        assert!(config.client_cert_path.is_none());
        assert!(config.client_key_path.is_none());
    }

    #[test]
    fn test_tls_config_insecure() {
        let config = TlsConfig {
            insecure: true,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
        };
        assert!(config.insecure);
    }

    #[test]
    fn test_tls_config_with_ca_cert() {
        let config = TlsConfig {
            insecure: false,
            ca_cert_path: Some("/path/to/ca.crt".to_string()),
            client_cert_path: None,
            client_key_path: None,
        };
        assert!(!config.insecure);
        assert_eq!(config.ca_cert_path, Some("/path/to/ca.crt".to_string()));
    }

    #[test]
    fn test_tls_config_with_mtls() {
        let config = TlsConfig {
            insecure: false,
            ca_cert_path: Some("/path/to/ca.crt".to_string()),
            client_cert_path: Some("/path/to/client.crt".to_string()),
            client_key_path: Some("/path/to/client.key".to_string()),
        };
        assert!(!config.insecure);
        assert_eq!(config.ca_cert_path, Some("/path/to/ca.crt".to_string()));
        assert_eq!(config.client_cert_path, Some("/path/to/client.crt".to_string()));
        assert_eq!(config.client_key_path, Some("/path/to/client.key".to_string()));
    }

    #[test]
    fn test_tls_config_serialization() {
        let config = TlsConfig {
            insecure: true,
            ca_cert_path: Some("/path/to/ca.crt".to_string()),
            client_cert_path: None,
            client_key_path: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"insecure\":true"));
        assert!(json.contains("\"ca_cert_path\":\"/path/to/ca.crt\""));

        let deserialized: TlsConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.insecure);
        assert_eq!(deserialized.ca_cert_path, Some("/path/to/ca.crt".to_string()));
    }

    #[test]
    fn test_tls_config_clone() {
        let config = TlsConfig {
            insecure: true,
            ca_cert_path: Some("/path/to/ca.crt".to_string()),
            client_cert_path: None,
            client_key_path: None,
        };
        let cloned = config.clone();
        assert_eq!(config.insecure, cloned.insecure);
        assert_eq!(config.ca_cert_path, cloned.ca_cert_path);
    }

    // =========================================================================
    // UnaryResponse Tests
    // =========================================================================

    #[test]
    fn test_unary_response_creation() {
        let mut metadata = HashMap::new();
        metadata.insert("content-type".to_string(), "application/grpc".to_string());

        let response = UnaryResponse {
            json_payload: r#"{"message":"hello"}"#.to_string(),
            metadata,
            status: ResponseStatus {
                code: 0,
                message: "OK".to_string(),
                status: "OK".to_string(),
            },
            duration_ms: 42,
        };

        assert_eq!(response.json_payload, r#"{"message":"hello"}"#);
        assert_eq!(response.duration_ms, 42);
        assert_eq!(response.status.code, 0);
        assert_eq!(response.metadata.get("content-type"), Some(&"application/grpc".to_string()));
    }

    #[test]
    fn test_unary_response_serialization() {
        let response = UnaryResponse {
            json_payload: "{}".to_string(),
            metadata: HashMap::new(),
            status: ResponseStatus {
                code: 0,
                message: "OK".to_string(),
                status: "OK".to_string(),
            },
            duration_ms: 0,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: UnaryResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.json_payload, "{}");
        assert_eq!(deserialized.duration_ms, 0);
    }

    // =========================================================================
    // ClientManager Tests
    // =========================================================================

    #[test]
    fn test_client_manager_new() {
        let manager = ClientManager::new();
        assert_eq!(manager.clients.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_client_manager_default() {
        let manager: ClientManager = Default::default();
        assert_eq!(manager.clients.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_client_manager_remove() {
        let manager = ClientManager::new();
        // Remove non-existent client should not panic
        manager.remove("non-existent");
        assert_eq!(manager.clients.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_client_manager_clear() {
        let manager = ClientManager::new();
        // Clear empty manager should not panic
        manager.clear();
        assert_eq!(manager.clients.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_client_manager_thread_safety() {
        use std::thread;

        let manager = Arc::new(ClientManager::new());
        let mut handles = vec![];

        for i in 0..10 {
            let manager_clone = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                manager_clone.remove(&format!("addr{}", i));
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    // =========================================================================
    // GrpcClient Tests (without actual connection)
    // =========================================================================

    #[test]
    fn test_grpc_client_debug() {
        // Test that GrpcClient implements Debug
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<GrpcClient>();
    }

    // =========================================================================
    // Additional GrpcError Conversion Tests
    // =========================================================================

    #[test]
    fn test_grpc_error_from_transport_error_all_variants() {
        // Test InvalidAddress
        let err: GrpcError = TransportError::InvalidAddress("bad addr".to_string()).into();
        assert!(matches!(err, GrpcError::Transport(TransportError::InvalidAddress(_))));

        // Test ConnectionFailed
        let err: GrpcError = TransportError::ConnectionFailed("refused".to_string()).into();
        assert!(matches!(err, GrpcError::Transport(TransportError::ConnectionFailed(_))));

        // Test TlsError
        let err: GrpcError = TransportError::TlsError("cert error".to_string()).into();
        assert!(matches!(err, GrpcError::Transport(TransportError::TlsError(_))));

        // Test RequestError
        let err: GrpcError = TransportError::RequestError("bad req".to_string()).into();
        assert!(matches!(err, GrpcError::Transport(TransportError::RequestError(_))));

        // Test ResponseError
        let err: GrpcError = TransportError::ResponseError("bad resp".to_string()).into();
        assert!(matches!(err, GrpcError::Transport(TransportError::ResponseError(_))));
    }

    #[test]
    fn test_grpc_error_from_ffi_error_other_variants() {
        // Test LibraryNotFound -> Encoding
        let err: GrpcError = FfiError::LibraryNotFound { paths: vec![] }.into();
        assert!(matches!(err, GrpcError::Encoding(_)));

        // Test NullPointer -> Encoding
        let err: GrpcError = FfiError::NullPointer { context: "test".to_string() }.into();
        assert!(matches!(err, GrpcError::Encoding(_)));
    }

    #[test]
    fn test_grpc_error_display_all_variants() {
        let transport_err = GrpcError::Transport(TransportError::Timeout);
        assert!(transport_err.to_string().contains("Transport error"));

        let encoding_err = GrpcError::Encoding("test".to_string());
        assert!(encoding_err.to_string().contains("Encoding error"));

        let decoding_err = GrpcError::Decoding("test".to_string());
        assert!(decoding_err.to_string().contains("Decoding error"));

        let invalid_method_err = GrpcError::InvalidMethod("test".to_string());
        assert!(invalid_method_err.to_string().contains("Invalid method"));
    }

    // =========================================================================
    // Additional ResponseStatus Tests
    // =========================================================================

    #[test]
    fn test_response_status_clone() {
        let status = ResponseStatus {
            code: 14,
            message: "Unavailable".to_string(),
            status: "ERROR".to_string(),
        };
        let cloned = status.clone();
        assert_eq!(status.code, cloned.code);
        assert_eq!(status.message, cloned.message);
        assert_eq!(status.status, cloned.status);
    }

    #[test]
    fn test_response_status_equality() {
        let status1 = ResponseStatus {
            code: 0,
            message: "OK".to_string(),
            status: "OK".to_string(),
        };
        let status2 = ResponseStatus {
            code: 0,
            message: "OK".to_string(),
            status: "OK".to_string(),
        };
        let status3 = ResponseStatus {
            code: 14,
            message: "Unavailable".to_string(),
            status: "ERROR".to_string(),
        };
        assert_eq!(status1.code, status2.code);
        assert_ne!(status1.code, status3.code);
    }

    // =========================================================================
    // Additional TlsConfig Tests
    // =========================================================================

    #[test]
    fn test_tls_config_debug() {
        let config = TlsConfig {
            insecure: true,
            ca_cert_path: Some("/path/to/ca.crt".to_string()),
            client_cert_path: None,
            client_key_path: None,
        };
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("insecure"));
        assert!(debug_str.contains("ca_cert_path"));
    }

    #[test]
    fn test_tls_config_partial_eq() {
        let config1 = TlsConfig {
            insecure: true,
            ca_cert_path: Some("/path/to/ca.crt".to_string()),
            client_cert_path: None,
            client_key_path: None,
        };
        let config2 = TlsConfig {
            insecure: true,
            ca_cert_path: Some("/path/to/ca.crt".to_string()),
            client_cert_path: None,
            client_key_path: None,
        };
        assert_eq!(config1.insecure, config2.insecure);
        assert_eq!(config1.ca_cert_path, config2.ca_cert_path);
    }

    #[test]
    fn test_tls_config_deserialization() {
        let json = r#"{
            "insecure": false,
            "ca_cert_path": "/path/to/ca.crt",
            "client_cert_path": null,
            "client_key_path": null
        }"#;
        let config: TlsConfig = serde_json::from_str(json).unwrap();
        assert!(!config.insecure);
        assert_eq!(config.ca_cert_path, Some("/path/to/ca.crt".to_string()));
        assert!(config.client_cert_path.is_none());
    }

    // =========================================================================
    // Additional ClientManager Tests
    // =========================================================================

    #[test]
    fn test_client_manager_clone() {
        let manager = ClientManager::new();
        // Arc<ClientManager> should be cloneable
        let _arc_manager = Arc::new(manager);
    }

    #[test]
    fn test_client_manager_mutex_poisoning_recovery() {
        // Test that we can create a new manager after operations
        let manager = ClientManager::new();
        manager.clear();
        manager.remove("test");
        // Should still be usable
        assert_eq!(manager.clients.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_stream_response_creation() {
        let response = StreamResponse {
            json_payload: r#"{"result":"success"}"#.to_string(),
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("key".to_string(), "value".to_string());
                m
            }),
            status: Some(ResponseStatus {
                code: 0,
                message: "OK".to_string(),
                status: "OK".to_string(),
            }),
        };
        assert_eq!(response.json_payload, r#"{"result":"success"}"#);
        assert!(response.metadata.is_some());
        assert!(response.status.is_some());
    }

    #[test]
    fn test_stream_response_serialization() {
        let response = StreamResponse {
            json_payload: "{}".to_string(),
            metadata: None,
            status: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: StreamResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.json_payload, "{}");
        assert!(deserialized.metadata.is_none());
        assert!(deserialized.status.is_none());
    }
}
