//! gRPC Transport handling
//!
//! This module provides HTTP/2 transport for gRPC calls using tonic.

#![allow(dead_code)]
use bytes::Buf;
use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;
use http::HeaderMap;
use http::Request;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tower::ServiceExt;
use http_body_util::BodyExt;

use super::{format_error_with_chain, metadata::headers_to_metadata};

/// CollectedBody 聚合一次 gRPC 响应体读取结果。
///
/// 之所以需要单独结构体，是因为 gRPC 语义把状态码放在 trailers，
/// 如果只读取 data 而忽略 trailers，会把很多服务端错误误判成成功。
#[derive(Debug)]
struct CollectedBody {
    data: Bytes,
    trailers: HeaderMap,
}

/// collect_body_data 负责读取 tonic body 的 data 与 trailers。
///
/// 读取策略：
/// 1) data frame 按顺序拼接到连续字节流；
/// 2) trailers frame 全量保留，后续用于解析 grpc-status/grpc-message；
/// 3) 任一 frame 读取失败都转换为 ResponseError，保证上层错误语义稳定。
async fn collect_body_data<B>(body: B) -> Result<CollectedBody, TransportError>
where
    B: http_body::Body + Unpin,
    B::Error: std::fmt::Display,
    B::Data: bytes::Buf,
{
    let mut result = Vec::new();
    let mut trailers = HeaderMap::new();
    let mut body = body;
    while let Some(frame_result) = body.frame().await {
        let frame = frame_result.map_err(|e| TransportError::ResponseError(format!("Failed to read response: {}", e)))?;
        if let Some(data) = frame.data_ref() {
            result.extend_from_slice(data.chunk());
        }

        if let Some(frame_trailers) = frame.trailers_ref() {
            for (key, value) in frame_trailers {
                trailers.append(key, value.clone());
            }
        }
    }

    Ok(CollectedBody {
        data: Bytes::from(result),
        trailers,
    })
}

/// encode_grpc_frame 把 protobuf message 打包成标准 gRPC 消息帧。
///
/// gRPC over HTTP/2 的消息体格式固定为：
/// - 1 字节压缩标记（0 = 未压缩，1 = 已压缩）
/// - 4 字节大端长度
/// - N 字节消息载荷
///
/// 当前客户端未启用请求压缩，因此压缩标记恒为 0。
fn encode_grpc_frame(payload: &[u8]) -> Result<Bytes, TransportError> {
    let payload_len = u32::try_from(payload.len()).map_err(|_| {
        TransportError::RequestError(format!(
            "Request payload too large: {} bytes exceeds gRPC frame limit",
            payload.len()
        ))
    })?;

    let mut framed = Vec::with_capacity(5 + payload.len());
    framed.push(0);
    framed.extend_from_slice(&payload_len.to_be_bytes());
    framed.extend_from_slice(payload);
    Ok(Bytes::from(framed))
}

/// decode_grpc_unary_response 从 gRPC 帧字节流中提取 unary 响应消息。
///
/// 处理规则：
/// 1) 空 body 返回空 payload（常见于仅返回非 0 grpc-status 的场景）；
/// 2) 必须至少包含完整 5 字节帧头，否则视为协议损坏；
/// 3) 仅支持未压缩帧，若服务端启用压缩会返回明确错误；
/// 4) unary 响应只允许一个消息帧，多个帧直接报错避免静默吞数据。
fn decode_grpc_unary_response(body: &[u8]) -> Result<Bytes, TransportError> {
    if body.is_empty() {
        return Ok(Bytes::new());
    }

    let mut offset = 0usize;
    let mut unary_payload: Option<Bytes> = None;

    while offset < body.len() {
        if body.len() - offset < 5 {
            return Err(TransportError::ResponseError(
                "Malformed gRPC frame: incomplete frame header".to_string(),
            ));
        }

        let compressed_flag = body[offset];
        if compressed_flag != 0 {
            return Err(TransportError::ResponseError(
                "Compressed gRPC response is not supported".to_string(),
            ));
        }

        let message_len = u32::from_be_bytes([
            body[offset + 1],
            body[offset + 2],
            body[offset + 3],
            body[offset + 4],
        ]) as usize;
        offset += 5;

        if body.len() - offset < message_len {
            return Err(TransportError::ResponseError(
                "Malformed gRPC frame: payload length exceeds available bytes".to_string(),
            ));
        }

        let message = Bytes::copy_from_slice(&body[offset..offset + message_len]);
        offset += message_len;

        if unary_payload.is_some() {
            return Err(TransportError::ResponseError(
                "Unary gRPC response contains multiple message frames".to_string(),
            ));
        }
        unary_payload = Some(message);
    }

    Ok(unary_payload.unwrap_or_default())
}

/// gRPC transport configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Connection timeout
    pub timeout: Duration,
    /// TLS configuration
    pub tls_config: Option<ClientTlsConfig>,
    /// Optional authority override for HTTP/2 `:authority` and TLS SNI
    pub authority: Option<String>,
    /// Whether to use insecure connection
    pub insecure: bool,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            tls_config: None,
            authority: None,
            insecure: false,
        }
    }
}

/// gRPC transport handle
#[derive(Debug, Clone)]
pub struct GrpcTransport {
    channel: Channel,
    config: TransportConfig,
}

impl GrpcTransport {
    /// build_origin_from_authority 根据 authority 构建 Endpoint origin。
    ///
    /// tonic `Endpoint::origin` 会同时影响：
    /// - HTTP/2 `:authority` 伪首部；
    /// - TLS SNI 主机名。
    ///
    /// 这可以解决“连接地址是 IP，但网关按域名路由/证书校验”的场景。
    fn build_origin_from_authority(
        authority: &str,
        insecure: bool,
    ) -> Result<http::Uri, TransportError> {
        let trimmed = authority.trim();
        if trimmed.is_empty() {
            return Err(TransportError::InvalidAddress(
                "Authority override is empty".to_string(),
            ));
        }

        let origin = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            trimmed.to_string()
        } else if insecure {
            format!("http://{}", trimmed)
        } else {
            format!("https://{}", trimmed)
        };

        origin
            .parse::<http::Uri>()
            .map_err(|error| TransportError::InvalidAddress(format!(
                "Invalid authority '{}': {}",
                trimmed, error
            )))
    }

    /// extract_server_name_from_authority 从 authority 解析 TLS SNI 的 server name。
    ///
    /// `openssl s_client -servername` 传入的是纯主机名（不含 scheme），
    /// 这里统一把 authority 解析为 URI 后取 host，避免把端口误传给 `domain_name`。
    fn extract_server_name_from_authority(
        authority: &str,
        insecure: bool,
    ) -> Result<String, TransportError> {
        let origin = Self::build_origin_from_authority(authority, insecure)?;
        origin.host().map(|host| host.to_string()).ok_or_else(|| {
            TransportError::InvalidAddress(format!(
                "Invalid authority '{}': missing host",
                authority.trim()
            ))
        })
    }

    /// Connect to a gRPC server
    pub async fn connect(address: &str, config: TransportConfig) -> Result<Self, TransportError> {
        let endpoint = if address.starts_with("http://") || address.starts_with("https://") {
            Endpoint::from_shared(address.to_string())
        } else {
            // Default to http:// for insecure, https:// otherwise
            let scheme = if config.insecure { "http" } else { "https" };
            Endpoint::from_shared(format!("{}://{}", scheme, address))
        }
        .map_err(|e| TransportError::InvalidAddress(e.to_string()))?;
        
        let endpoint = endpoint
            .timeout(config.timeout)
            .connect_timeout(Duration::from_secs(10));

        let resolved_server_name = if let Some(authority) = config.authority.as_deref() {
            Some(Self::extract_server_name_from_authority(authority, config.insecure)?)
        } else {
            None
        };

        let endpoint = if let Some(authority) = config.authority.as_deref() {
            let origin = Self::build_origin_from_authority(authority, config.insecure)?;
            endpoint.origin(origin)
        } else {
            endpoint
        };
        
        let endpoint = if let Some(tls) = &config.tls_config {
            let tls = if let Some(server_name) = resolved_server_name.as_deref() {
                tls.clone().domain_name(server_name.to_string())
            } else {
                tls.clone()
            };
            endpoint.tls_config(tls).map_err(|e| {
                TransportError::TlsError(format!("Failed to configure TLS: {}", e))
            })?
        } else if !config.insecure {
            // 默认启用 tonic 支持的根证书集合（native/webpki）。
            let tls = if let Some(server_name) = resolved_server_name.as_deref() {
                ClientTlsConfig::new()
                    .with_enabled_roots()
                    .domain_name(server_name.to_string())
            } else {
                ClientTlsConfig::new().with_enabled_roots()
            };
            endpoint.tls_config(tls).map_err(|e| {
                TransportError::TlsError(format!("Failed to configure TLS: {}", e))
            })?
        } else {
            endpoint
        };
        
        let channel = endpoint.connect().await.map_err(|e| {
            let error_report = format_error_with_chain(&e);
            log::error!(
                "gRPC transport connect failed: address={}, insecure={}, diagnostics={}",
                address,
                config.insecure,
                error_report
            );
            TransportError::ConnectionFailed(format!("Failed to connect: {}", error_report))
        })?;
        
        Ok(Self { channel, config })
    }
    
    /// Perform a unary gRPC call
    pub async fn unary_call(
        &self,
        method: &str,
        request_body: Bytes,
        metadata: &HashMap<String, String>,
        _timeout: Option<Duration>,
    ) -> Result<UnaryResponse, TransportError> {
        let start_time = std::time::Instant::now();
        
        // Parse method path (format: "/Service/Method" or "Service/Method")
        let method_path = if method.starts_with('/') {
            method.to_string()
        } else {
            format!("/{}", method)
        };
        
        // Build request
        let mut request_builder = Request::builder()
            .method("POST")
            .uri(method_path.clone())
            .header("content-type", "application/grpc")
            .header("te", "trailers");
        
        // Add metadata headers
        for (key, value) in metadata {
            if let (Ok(header_name), Ok(header_value)) = (
                http::HeaderName::from_bytes(key.as_bytes()),
                http::HeaderValue::from_str(value)
            ) {
                request_builder = request_builder.header(header_name, header_value);
            }
        }
        
        // Create a gRPC framed request body and box it for tonic
        let framed_request_body = encode_grpc_frame(request_body.as_ref())?;
        let body = http_body_util::Full::new(framed_request_body);
        let boxed_body = body.map_err(|e: std::convert::Infallible| match e {}).boxed_unsync();
        let request = request_builder
            .body(boxed_body)
            .map_err(|e| {
                let error_report = format_error_with_chain(&e);
                log::error!(
                    "gRPC unary request build failed: method_path={}, diagnostics={}",
                    method_path,
                    error_report
                );
                TransportError::RequestError(format!(
                    "Request build failed: {}; method_path={}",
                    error_report, method_path
                ))
            })?;

        let mut metadata_keys: Vec<&str> = metadata.keys().map(String::as_str).collect();
        metadata_keys.sort_unstable();
        let request_context = format!(
            "method_path={}, body_len={}, metadata_keys={:?}",
            method_path,
            request_body.len(),
            metadata_keys
        );

        // Perform the call using tonic's channel
        let response = self
            .channel
            .clone()
            .oneshot(request)
            .await
            .map_err(|e| {
                let error_report = format_error_with_chain(&e);
                log::error!(
                    "gRPC unary request failed: diagnostics={}, context={}",
                    error_report,
                    request_context
                );
                TransportError::RequestError(format!(
                    "Request failed: {}; context={}",
                    error_report, request_context
                ))
            })?;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Extract response headers
        let response_headers = response.headers().clone();

        // Get response body using tonic's body handling
        let body = response.into_body();
        let collected_body = Self::collect_body(body).await?;

        // Merge headers and trailers as metadata for frontend visibility.
        let mut metadata = headers_to_metadata(&response_headers);
        for (key, value) in &collected_body.trailers {
            if let Ok(value_str) = value.to_str() {
                metadata.insert(key.as_str().to_string(), value_str.to_string());
            }
        }

        let body_bytes = decode_grpc_unary_response(collected_body.data.as_ref())?;

        // Parse gRPC status from headers or trailers
        let grpc_status = collected_body
            .trailers
            .get("grpc-status")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<i32>().ok())
            .or_else(|| {
                response_headers
                    .get("grpc-status")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<i32>().ok())
            })
            .unwrap_or(0);

        let grpc_message = collected_body
            .trailers
            .get("grpc-message")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let grpc_message = grpc_message.or_else(|| {
            response_headers
                .get("grpc-message")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        });
        
        let status = if grpc_status == 0 {
            GrpcStatus::Ok
        } else {
            GrpcStatus::Error {
                code: grpc_status,
                message: grpc_message.unwrap_or_else(|| "Unknown error".to_string()),
            }
        };
        
        Ok(UnaryResponse {
            body: body_bytes,
            metadata,
            status,
            duration_ms,
        })
    }
    
    /// Check if the connection is ready
    pub async fn is_ready(&self) -> bool {
        // Clone the channel to check readiness
        let mut channel = self.channel.clone();
        channel.ready().await.is_ok()
    }

    /// Get the underlying channel
    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    /// Collect all data from a tonic body
    async fn collect_body<B>(body: B) -> Result<CollectedBody, TransportError>
    where
        B: http_body::Body + Unpin,
        B::Error: std::fmt::Display,
        B::Data: bytes::Buf,
    {
        collect_body_data(body).await
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

/// Simplified status for responses
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

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // GrpcStatus Tests
    // =========================================================================

    #[test]
    fn test_status_ok() {
        let status = GrpcStatus::Ok;
        assert!(status.is_ok());
        assert_eq!(status.code(), 0);
        assert_eq!(status.as_str(), "OK");
    }

    #[test]
    fn test_status_error() {
        let status = GrpcStatus::Error {
            code: 14,
            message: "Unavailable".to_string(),
        };
        assert!(!status.is_ok());
        assert_eq!(status.code(), 14);
        assert_eq!(status.as_str(), "ERROR");
    }

    #[test]
    fn test_status_error_various_codes() {
        // Test various gRPC status codes
        let codes = vec![
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
            let status = GrpcStatus::Error {
                code,
                message: message.to_string(),
            };
            assert!(!status.is_ok());
            assert_eq!(status.code(), code);
            assert_eq!(status.as_str(), "ERROR");
        }
    }

    #[test]
    fn test_status_clone() {
        let status = GrpcStatus::Error {
            code: 5,
            message: "Not found".to_string(),
        };
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }

    #[test]
    fn test_status_equality() {
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
    // TransportConfig Tests
    // =========================================================================

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.tls_config.is_none());
        assert!(config.authority.is_none());
        assert!(!config.insecure);
    }

    #[test]
    fn test_transport_config_with_timeout() {
        let config = TransportConfig {
            timeout: Duration::from_secs(60),
            tls_config: None,
            authority: None,
            insecure: false,
        };
        assert_eq!(config.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_transport_config_insecure() {
        let config = TransportConfig {
            timeout: Duration::from_secs(30),
            tls_config: None,
            authority: None,
            insecure: true,
        };
        assert!(config.insecure);
    }

    #[test]
    fn test_transport_config_clone() {
        let config = TransportConfig::default();
        let cloned = config.clone();
        assert_eq!(config.timeout, cloned.timeout);
        assert_eq!(config.insecure, cloned.insecure);
    }

    // =========================================================================
    // TransportError Tests
    // =========================================================================

    #[test]
    fn test_transport_error_display_invalid_address() {
        let err = TransportError::InvalidAddress("bad address".to_string());
        assert_eq!(err.to_string(), "Invalid address: bad address");
    }

    #[test]
    fn test_transport_error_display_connection_failed() {
        let err = TransportError::ConnectionFailed("connection refused".to_string());
        assert_eq!(err.to_string(), "Connection failed: connection refused");
    }

    #[test]
    fn test_transport_error_display_tls_error() {
        let err = TransportError::TlsError("cert invalid".to_string());
        assert_eq!(err.to_string(), "TLS error: cert invalid");
    }

    #[test]
    fn test_transport_error_display_request_error() {
        let err = TransportError::RequestError("bad request".to_string());
        assert_eq!(err.to_string(), "Request error: bad request");
    }

    #[test]
    fn test_transport_error_display_response_error() {
        let err = TransportError::ResponseError("server error".to_string());
        assert_eq!(err.to_string(), "Response error: server error");
    }

    #[test]
    fn test_transport_error_display_timeout() {
        let err = TransportError::Timeout;
        assert_eq!(err.to_string(), "Request timed out");
    }

    #[test]
    fn test_transport_error_clone() {
        let err = TransportError::ConnectionFailed("test".to_string());
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }

    #[test]
    fn test_transport_error_error_trait() {
        let err = TransportError::Timeout;
        let err_ref: &dyn std::error::Error = &err;
        assert!(err_ref.source().is_none());
    }

    // =========================================================================
    // UnaryResponse Tests
    // =========================================================================

    #[test]
    fn test_unary_response_creation() {
        let mut metadata = HashMap::new();
        metadata.insert("content-type".to_string(), "application/grpc".to_string());

        let response = UnaryResponse {
            body: Bytes::from_static(b"test data"),
            metadata,
            status: GrpcStatus::Ok,
            duration_ms: 42,
        };

        assert_eq!(response.body, Bytes::from_static(b"test data"));
        assert_eq!(response.duration_ms, 42);
        assert!(response.status.is_ok());
        assert_eq!(response.metadata.get("content-type"), Some(&"application/grpc".to_string()));
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
        let response = UnaryResponse {
            body: Bytes::from_static(b"data"),
            metadata: HashMap::new(),
            status: GrpcStatus::Ok,
            duration_ms: 10,
        };
        let cloned = response.clone();
        assert_eq!(response.body, cloned.body);
        assert_eq!(response.duration_ms, cloned.duration_ms);
    }

    // =========================================================================
    // GrpcTransport Tests (without actual connection)
    // =========================================================================

    #[test]
    fn test_grpc_transport_debug() {
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<GrpcTransport>();
    }

    #[test]
    fn test_grpc_transport_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<GrpcTransport>();
    }

    // =========================================================================
    // Additional TransportError Tests
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

    // =========================================================================
    // Additional GrpcStatus Tests
    // =========================================================================

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

    // =========================================================================
    // Additional TransportConfig Tests
    // =========================================================================

    #[test]
    fn test_transport_config_custom_values() {
        let config = TransportConfig {
            timeout: Duration::from_millis(500),
            tls_config: None,
            authority: None,
            insecure: true,
        };
        assert_eq!(config.timeout, Duration::from_millis(500));
        assert!(config.insecure);
    }

    #[test]
    fn test_transport_config_with_tls() {
        // Note: We can't easily create a ClientTlsConfig, but we can verify the field exists
        let config = TransportConfig {
            timeout: Duration::from_secs(30),
            tls_config: None, // Would be Some(tls_config) in real usage
            authority: None,
            insecure: false,
        };
        assert!(config.tls_config.is_none());
    }

    #[test]
    fn test_build_origin_from_authority_for_tls() {
        let origin = GrpcTransport::build_origin_from_authority("api.example.com", false)
            .expect("TLS 场景应能构建 https origin");
        assert_eq!(origin, "https://api.example.com".parse::<http::Uri>().unwrap());
    }

    #[test]
    fn test_build_origin_from_authority_for_plaintext() {
        let origin = GrpcTransport::build_origin_from_authority("127.0.0.1:30080", true)
            .expect("明文场景应能构建 http origin");
        assert_eq!(origin, "http://127.0.0.1:30080".parse::<http::Uri>().unwrap());
    }

    #[test]
    fn test_extract_server_name_from_authority_strips_port() {
        let server_name = GrpcTransport::extract_server_name_from_authority(
            "nona-sit.ngnet.com.cn:31443",
            false,
        )
        .expect("应提取域名作为 SNI server name");
        assert_eq!(server_name, "nona-sit.ngnet.com.cn");
    }

    #[test]
    fn test_extract_server_name_from_authority_keeps_host_only() {
        let server_name = GrpcTransport::extract_server_name_from_authority(
            "https://api.example.com:8443",
            false,
        )
        .expect("带 scheme 的 authority 也应可解析");
        assert_eq!(server_name, "api.example.com");
    }

    // =========================================================================
    // Additional UnaryResponse Tests
    // =========================================================================

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
    fn test_unary_response_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("grpc-status".to_string(), "0".to_string());
        metadata.insert("content-type".to_string(), "application/grpc".to_string());
        metadata.insert("custom-header".to_string(), "custom-value".to_string());

        let response = UnaryResponse {
            body: Bytes::from_static(b"test"),
            metadata,
            status: GrpcStatus::Ok,
            duration_ms: 10,
        };

        assert_eq!(response.metadata.get("grpc-status"), Some(&"0".to_string()));
        assert_eq!(response.metadata.get("custom-header"), Some(&"custom-value".to_string()));
    }

    #[test]
    fn test_unary_response_error_status() {
        let response = UnaryResponse {
            body: Bytes::from_static(b"error details"),
            metadata: HashMap::new(),
            status: GrpcStatus::Error { code: 14, message: "Unavailable".to_string() },
            duration_ms: 5000,
        };
        assert!(!response.status.is_ok());
        assert_eq!(response.status.code(), 14);
    }

    // =========================================================================
    // BodyData Trait Tests (via UnaryResponse body handling)
    // =========================================================================

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

    #[test]
    fn test_encode_grpc_frame_wraps_payload() {
        let payload = b"abc";
        let framed = encode_grpc_frame(payload).expect("encode_grpc_frame should succeed");

        assert_eq!(framed.len(), payload.len() + 5);
        assert_eq!(framed[0], 0);
        assert_eq!(&framed[1..5], &[0, 0, 0, 3]);
        assert_eq!(&framed[5..], payload);
    }

    #[test]
    fn test_decode_grpc_unary_response_extracts_payload() {
        let framed = Bytes::from_static(&[0, 0, 0, 0, 3, b'a', b'b', b'c']);
        let decoded = decode_grpc_unary_response(framed.as_ref())
            .expect("decode_grpc_unary_response should succeed");
        assert_eq!(decoded, Bytes::from_static(b"abc"));
    }

    #[test]
    fn test_decode_grpc_unary_response_rejects_incomplete_header() {
        let framed = Bytes::from_static(&[0, 0, 0, 0]);
        let err = decode_grpc_unary_response(framed.as_ref())
            .expect_err("expected malformed frame header error");
        assert!(err.to_string().contains("Malformed gRPC frame"));
    }

    #[test]
    fn test_decode_grpc_unary_response_rejects_compressed_frame() {
        let framed = Bytes::from_static(&[1, 0, 0, 0, 0]);
        let err = decode_grpc_unary_response(framed.as_ref())
            .expect_err("expected compressed frame unsupported error");
        assert!(err.to_string().contains("Compressed gRPC response is not supported"));
    }
}
