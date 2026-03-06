//! gRPC Streaming Implementation
//!
//! This module provides native Rust gRPC streaming support using tonic.
//! It integrates with the FFI bridge for proto parsing and message encoding/decoding.

#![allow(dead_code)]
use bytes::Buf;
use std::collections::HashMap;
use std::sync::Arc;

use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::task::AbortHandle;
use tonic::transport::Channel;

use super::format_error_with_chain;
use crate::error::{AppError, Result};

/// Type of gRPC stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamType {
    /// Server streaming: Server -> Client
    #[serde(rename = "server")]
    ServerStreaming,
    /// Client streaming: Client -> Server
    #[serde(rename = "client")]
    ClientStreaming,
    /// Bidirectional streaming: Both directions
    #[serde(rename = "bidi")]
    Bidirectional,
}

impl StreamType {
    /// Returns true if this stream type receives messages from server
    pub fn has_server_stream(&self) -> bool {
        matches!(self, StreamType::ServerStreaming | StreamType::Bidirectional)
    }

    /// Returns true if this stream type sends messages to server
    pub fn has_client_stream(&self) -> bool {
        matches!(self, StreamType::ClientStreaming | StreamType::Bidirectional)
    }
}

impl std::fmt::Display for StreamType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamType::ServerStreaming => write!(f, "server"),
            StreamType::ClientStreaming => write!(f, "client"),
            StreamType::Bidirectional => write!(f, "bidi"),
        }
    }
}

/// Events emitted during streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    /// Message received from server
    #[serde(rename = "message")]
    Message {
        #[serde(rename = "streamId")]
        stream_id: String,
        data: serde_json::Value,
    },
    /// Metadata received from server
    #[serde(rename = "metadata")]
    Metadata {
        #[serde(rename = "streamId")]
        stream_id: String,
        metadata: HashMap<String, String>,
    },
    /// Error occurred during streaming
    #[serde(rename = "error")]
    Error {
        #[serde(rename = "streamId")]
        stream_id: String,
        message: String,
    },
    /// Stream ended successfully
    #[serde(rename = "end")]
    End {
        #[serde(rename = "streamId")]
        stream_id: String,
    },
}

impl StreamEvent {
    /// Get the stream ID associated with this event
    pub fn stream_id(&self) -> &str {
        match self {
            StreamEvent::Message { stream_id, .. } => stream_id,
            StreamEvent::Metadata { stream_id, .. } => stream_id,
            StreamEvent::Error { stream_id, .. } => stream_id,
            StreamEvent::End { stream_id } => stream_id,
        }
    }
}

/// Handle to identify and control an active stream
#[derive(Debug, Clone)]
pub struct StreamHandle {
    pub id: String,
    pub stream_type: StreamType,
}

/// Internal stream state
struct ActiveStream {
    handle: StreamHandle,
    /// Channel for sending messages to the stream (client/bidi only)
    request_tx: Option<mpsc::Sender<String>>,
    /// Abort handle for the stream task
    abort_handle: AbortHandle,
}

/// Manages active gRPC streams
pub struct StreamManager {
    /// Active streams indexed by stream ID
    streams: Arc<RwLock<HashMap<String, ActiveStream>>>,
    /// gRPC channel for connections
    channel: Arc<RwLock<Option<Channel>>>,
    /// 共享 FFI codec，必须与 grpc_connect 加载描述时保持同一实例。
    codec: Arc<RwLock<Option<Arc<crate::ffi::CodecBridge>>>>,
}

impl StreamManager {
    /// Create a new StreamManager
    pub fn new() -> Self {
        Self {
            streams: Arc::new(RwLock::new(HashMap::new())),
            channel: Arc::new(RwLock::new(None)),
            codec: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the gRPC channel for connections
    pub async fn set_channel(&self, channel: Channel) {
        let mut ch = self.channel.write().await;
        *ch = Some(channel);
    }

    /// Clear the gRPC channel
    pub async fn clear_channel(&self) {
        let mut ch = self.channel.write().await;
        *ch = None;
    }

    /// Set the shared FFI codec used for stream encode/decode.
    ///
    /// 必须复用 `grpc_connect` 已加载描述信息的同一份 codec，
    /// 否则流式调用会出现“服务可见但方法找不到”的状态不一致问题。
    pub async fn set_codec(&self, codec: Arc<crate::ffi::CodecBridge>) {
        let mut shared_codec = self.codec.write().await;
        *shared_codec = Some(codec);
    }

    /// Clear the shared FFI codec
    pub async fn clear_codec(&self) {
        let mut shared_codec = self.codec.write().await;
        *shared_codec = None;
    }

    /// Check if a codec is available
    pub async fn has_codec(&self) -> bool {
        let shared_codec = self.codec.read().await;
        shared_codec.is_some()
    }

    /// Check if a channel is available
    pub async fn has_channel(&self) -> bool {
        let ch = self.channel.read().await;
        ch.is_some()
    }

    /// Start a new streaming call
    ///
    /// # Arguments
    /// * `method` - The gRPC method path (e.g., "package.Service/Method")
    /// * `stream_type` - Type of streaming (server, client, bidi)
    /// * `initial_request` - Initial request message for server streaming
    /// * `metadata` - gRPC metadata to send with the request
    /// * `event_tx` - Channel to send stream events back to the caller
    pub async fn start_stream(
        &self,
        method: &str,
        stream_type: StreamType,
        initial_request: Option<String>,
        metadata: HashMap<String, String>,
        event_tx: mpsc::Sender<StreamEvent>,
    ) -> Result<StreamHandle> {
        let channel = {
            let ch = self.channel.read().await;
            ch.clone().ok_or_else(|| {
                AppError::GrpcConnectionFailed(
                    "No gRPC connection available. Call grpc_connect first.".to_string(),
                )
            })?
        };

        let codec = {
            let shared_codec = self.codec.read().await;
            shared_codec.clone().ok_or_else(|| {
                AppError::GrpcConnectionFailed(
                    "No gRPC codec available. Call grpc_connect first.".to_string(),
                )
            })?
        };

        let stream_id = uuid::Uuid::new_v4().to_string();
        let handle = StreamHandle {
            id: stream_id.clone(),
            stream_type,
        };

        // Create channels for bidirectional communication
        let (request_tx, request_rx) = mpsc::channel::<String>(100);

        // Spawn the appropriate stream task based on type
        let abort_handle = match stream_type {
            StreamType::ServerStreaming => {
                let task = tokio::spawn(run_server_stream(
                    channel,
                    codec,
                    method.to_string(),
                    initial_request,
                    metadata,
                    stream_id.clone(),
                    event_tx,
                ));
                task.abort_handle()
            }
            StreamType::ClientStreaming => {
                let task = tokio::spawn(run_client_stream(
                    channel,
                    codec,
                    method.to_string(),
                    metadata,
                    stream_id.clone(),
                    request_rx,
                    event_tx,
                ));
                task.abort_handle()
            }
            StreamType::Bidirectional => {
                let task = tokio::spawn(run_bidi_stream(
                    channel,
                    codec,
                    method.to_string(),
                    metadata,
                    stream_id.clone(),
                    request_rx,
                    event_tx,
                ));
                task.abort_handle()
            }
        };

        // Register the stream
        let active_stream = ActiveStream {
            handle: handle.clone(),
            request_tx: if stream_type.has_client_stream() {
                Some(request_tx)
            } else {
                None
            },
            abort_handle,
        };

        {
            let mut streams = self.streams.write().await;
            streams.insert(stream_id, active_stream);
        }

        Ok(handle)
    }

    /// Send a message to a client or bidirectional stream
    pub async fn send_message(&self, stream_id: &str, json_payload: &str) -> Result<()> {
        let request_tx = {
            let streams = self.streams.read().await;
            let stream = streams.get(stream_id).ok_or_else(|| {
                AppError::GrpcStreamFailed(format!("Stream {} not found", stream_id))
            })?;

            if stream.handle.stream_type == StreamType::ServerStreaming {
                return Err(AppError::GrpcStreamFailed(
                    "Cannot send messages to server streaming call".to_string(),
                ));
            }

            stream.request_tx.clone().ok_or_else(|| {
                AppError::GrpcStreamFailed("Stream send channel not available".to_string())
            })?
        };

        request_tx
            .send(json_payload.to_string())
            .await
            .map_err(|_| AppError::GrpcStreamFailed("Failed to send message to stream".to_string()))?;

        Ok(())
    }

    /// Signal end of stream (half-close for client/bidi streams)
    pub async fn end_stream(&self, stream_id: &str) -> Result<()> {
        // Drop the sender to signal end of stream
        let mut streams = self.streams.write().await;
        let stream = streams.get_mut(stream_id).ok_or_else(|| {
            AppError::GrpcStreamFailed(format!("Stream {} not found", stream_id))
        })?;

        // Drop the sender channel to signal end
        stream.request_tx = None;

        Ok(())
    }

    /// Cancel a stream and clean up resources
    pub async fn cancel_stream(&self, stream_id: &str) -> Result<()> {
        let abort_handle = {
            let mut streams = self.streams.write().await;
            let stream = streams.remove(stream_id).ok_or_else(|| {
                AppError::GrpcStreamFailed(format!("Stream {} not found", stream_id))
            })?;
            stream.abort_handle
        };

        // Abort the stream task
        abort_handle.abort();

        Ok(())
    }

    /// Check if a stream exists
    pub async fn stream_exists(&self, stream_id: &str) -> bool {
        let streams = self.streams.read().await;
        streams.contains_key(stream_id)
    }

    /// Get the number of active streams
    pub async fn active_stream_count(&self) -> usize {
        let streams = self.streams.read().await;
        streams.len()
    }
}

impl Default for StreamManager {
    fn default() -> Self {
        Self::new()
    }
}

// ===== Server Streaming Implementation =====

async fn run_server_stream(
    channel: Channel,
    codec: Arc<crate::ffi::CodecBridge>,
    method: String,
    initial_request: Option<String>,
    metadata: HashMap<String, String>,
    stream_id: String,
    event_tx: mpsc::Sender<StreamEvent>,
) {
    let result = execute_server_stream(
        channel,
        codec,
        &method,
        initial_request,
        metadata,
        &stream_id,
        &event_tx,
    )
    .await;

    // Send end or error event
    match result {
        Ok(()) => {
            let _ = event_tx
                .send(StreamEvent::End {
                    stream_id: stream_id.clone(),
                })
                .await;
        }
        Err(e) => {
            let _ = event_tx
                .send(StreamEvent::Error {
                    stream_id: stream_id.clone(),
                    message: e.to_string(),
                })
                .await;
        }
    }
}

async fn execute_server_stream(
    channel: Channel,
    codec: Arc<crate::ffi::CodecBridge>,
    method: &str,
    initial_request: Option<String>,
    metadata: HashMap<String, String>,
    stream_id: &str,
    event_tx: &mpsc::Sender<StreamEvent>,
) -> Result<()> {
    use tower::ServiceExt;

    // Get the request JSON
    let request_json = initial_request.unwrap_or_else(|| "{}".to_string());

    // Encode the request
    let wire_data = codec
        .encode_request(method, &request_json)
        .map_err(|e| AppError::GrpcStreamFailed(format!("Failed to encode request: {}", e)))?;

    // Parse method path (format: "/Service/Method" or "Service/Method")
    let method_path = if method.starts_with('/') {
        method.to_string()
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
            http::HeaderValue::from_str(value),
        ) {
            request_builder = request_builder.header(header_name, header_value);
        }
    }

    let wire_data_len = wire_data.len();
    let mut metadata_keys: Vec<&str> = metadata.keys().map(String::as_str).collect();
    metadata_keys.sort_unstable();
    let request_context = format!(
        "stream_id={}, method_path={}, body_len={}, metadata_keys={:?}",
        stream_id,
        method_path,
        wire_data_len,
        metadata_keys
    );

    // Create body using http_body_util::Full and box it for tonic
    let body = http_body_util::Full::new(bytes::Bytes::from(wire_data));
    let boxed_body = body.map_err(|e: std::convert::Infallible| match e {}).boxed_unsync();

    let request = request_builder
        .body(boxed_body)
        .map_err(|e| {
            let error_report = format_error_with_chain(&e);
            log::error!(
                "gRPC server stream request build failed: diagnostics={}, context={}",
                error_report,
                request_context
            );
            AppError::GrpcStreamFailed(format!(
                "Failed to build request: {}; context={}",
                error_report, request_context
            ))
        })?;

    // Perform the streaming call using tower service
    let response = channel
        .oneshot(request)
        .await
        .map_err(|e| {
            let error_report = format_error_with_chain(&e);
            log::error!(
                "gRPC server stream request failed: diagnostics={}, context={}",
                error_report,
                request_context
            );
            AppError::GrpcStreamFailed(format!(
                "Request failed: {}; context={}",
                error_report, request_context
            ))
        })?;

    // Extract response headers
    let response_headers = response.headers().clone();
    let response_metadata = crate::grpc::metadata::headers_to_metadata(&response_headers);

    // Send metadata event
    if !response_metadata.is_empty() {
        let _ = event_tx
            .send(StreamEvent::Metadata {
                stream_id: stream_id.to_string(),
                metadata: response_metadata,
            })
            .await;
    }

    // Get response body stream
    let body = response.into_body();

    // Stream responses using http_body::Body trait
    stream_body(body, method, codec.as_ref(), stream_id, event_tx).await
}

/// Stream body chunks and decode them
async fn stream_body<B>(
    mut body: B,
    method: &str,
    codec: &crate::ffi::CodecBridge,
    stream_id: &str,
    event_tx: &mpsc::Sender<StreamEvent>,
) -> Result<()>
where
    B: http_body::Body + Unpin,
    B::Error: std::fmt::Display,
    B::Data: bytes::Buf,
{
    // Stream responses
    while let Some(frame_result) = body.frame().await {
        let frame = frame_result.map_err(|e| {
            AppError::GrpcStreamFailed(format!("Failed to read response chunk: {}", e))
        })?;

        // Get data from frame if it's a data frame
        let chunk = match frame.data_ref() {
            Some(data) => data.chunk(),
            None => continue,
        };

        if chunk.is_empty() {
            continue;
        }

        // Decode response from wire format to JSON
        match codec.decode_response(method, chunk) {
            Ok(json_payload) => {
                // Parse JSON
                match serde_json::from_str::<serde_json::Value>(&json_payload) {
                    Ok(data) => {
                        let _ = event_tx
                            .send(StreamEvent::Message {
                                stream_id: stream_id.to_string(),
                                data,
                            })
                            .await;
                    }
                    Err(e) => {
                        return Err(AppError::GrpcStreamFailed(format!(
                            "Failed to parse JSON response: {}",
                            e
                        )));
                    }
                }
            }
            Err(e) => {
                return Err(AppError::GrpcStreamFailed(format!(
                    "Failed to decode response: {}",
                    e
                )));
            }
        }
    }

    Ok(())
}

// ===== Client Streaming Implementation =====

async fn run_client_stream(
    channel: Channel,
    codec: Arc<crate::ffi::CodecBridge>,
    method: String,
    metadata: HashMap<String, String>,
    stream_id: String,
    request_rx: mpsc::Receiver<String>,
    event_tx: mpsc::Sender<StreamEvent>,
) {
    let result = execute_client_stream(
        channel,
        codec,
        &method,
        metadata,
        &stream_id,
        request_rx,
        &event_tx,
    )
    .await;

    // Send end or error event
    match result {
        Ok(response_data) => {
            // Send the final response as a message
            let _ = event_tx
                .send(StreamEvent::Message {
                    stream_id: stream_id.clone(),
                    data: response_data,
                })
                .await;

            let _ = event_tx
                .send(StreamEvent::End {
                    stream_id: stream_id.clone(),
                })
                .await;
        }
        Err(e) => {
            let _ = event_tx
                .send(StreamEvent::Error {
                    stream_id: stream_id.clone(),
                    message: e.to_string(),
                })
                .await;
        }
    }
}

async fn execute_client_stream(
    channel: Channel,
    codec: Arc<crate::ffi::CodecBridge>,
    method: &str,
    metadata: HashMap<String, String>,
    stream_id: &str,
    mut request_rx: mpsc::Receiver<String>,
    event_tx: &mpsc::Sender<StreamEvent>,
) -> Result<serde_json::Value> {
    use tower::ServiceExt;

    // Parse method path (format: "/Service/Method" or "Service/Method")
    let method_path = if method.starts_with('/') {
        method.to_string()
    } else {
        format!("/{}", method)
    };

    // Collect all request messages from the channel
    let mut request_messages: Vec<bytes::Bytes> = Vec::new();

    while let Some(json_payload) = request_rx.recv().await {
        // Encode each request message
        let wire_data = codec
            .encode_request(method, &json_payload)
            .map_err(|e| AppError::GrpcStreamFailed(format!("Failed to encode request: {}", e)))?;
        request_messages.push(bytes::Bytes::from(wire_data));
    }

    // Combine all messages into a single body (gRPC client streaming sends multiple frames)
    let combined_data: bytes::Bytes = request_messages.into_iter().flatten().collect();

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
            http::HeaderValue::from_str(value),
        ) {
            request_builder = request_builder.header(header_name, header_value);
        }
    }

    let combined_data_len = combined_data.len();
    let mut metadata_keys: Vec<&str> = metadata.keys().map(String::as_str).collect();
    metadata_keys.sort_unstable();
    let request_context = format!(
        "stream_id={}, method_path={}, body_len={}, metadata_keys={:?}",
        stream_id,
        method_path,
        combined_data_len,
        metadata_keys
    );

    // Create body using http_body_util::Full
    let body = http_body_util::Full::new(combined_data);
    let boxed_body = body.map_err(|e: std::convert::Infallible| match e {}).boxed_unsync();

    let request = request_builder
        .body(boxed_body)
        .map_err(|e| {
            let error_report = format_error_with_chain(&e);
            log::error!(
                "gRPC client stream request build failed: diagnostics={}, context={}",
                error_report,
                request_context
            );
            AppError::GrpcStreamFailed(format!(
                "Failed to build request: {}; context={}",
                error_report, request_context
            ))
        })?;

    // Perform the streaming call using tower service
    let response = channel
        .oneshot(request)
        .await
        .map_err(|e| {
            let error_report = format_error_with_chain(&e);
            log::error!(
                "gRPC client stream request failed: diagnostics={}, context={}",
                error_report,
                request_context
            );
            AppError::GrpcStreamFailed(format!(
                "Request failed: {}; context={}",
                error_report, request_context
            ))
        })?;

    // Extract response headers
    let response_headers = response.headers().clone();
    let response_metadata = crate::grpc::metadata::headers_to_metadata(&response_headers);

    // Send metadata event
    if !response_metadata.is_empty() {
        let _ = event_tx
            .send(StreamEvent::Metadata {
                stream_id: stream_id.to_string(),
                metadata: response_metadata,
            })
            .await;
    }

    // Get response body and decode the single response
    let body = response.into_body();

    // Collect all response data
    let mut response_data: Vec<u8> = Vec::new();
    let mut body_stream = body;

    while let Some(frame_result) = body_stream.frame().await {
        let frame = frame_result.map_err(|e| {
            AppError::GrpcStreamFailed(format!("Failed to read response chunk: {}", e))
        })?;

        if let Some(data) = frame.data_ref() {
            response_data.extend_from_slice(data.chunk());
        }
    }

    // Decode the response
    if response_data.is_empty() {
        return Ok(serde_json::Value::Null);
    }

    let json_response = codec
        .decode_response(method, &response_data)
        .map_err(|e| AppError::GrpcStreamFailed(format!("Failed to decode response: {}", e)))?;

    serde_json::from_str(&json_response).map_err(|e| {
        AppError::GrpcStreamFailed(format!("Failed to parse JSON response: {}", e))
    })
}

// ===== Bidirectional Streaming Implementation =====

async fn run_bidi_stream(
    channel: Channel,
    codec: Arc<crate::ffi::CodecBridge>,
    method: String,
    metadata: HashMap<String, String>,
    stream_id: String,
    request_rx: mpsc::Receiver<String>,
    event_tx: mpsc::Sender<StreamEvent>,
) {
    let result = execute_bidi_stream(
        channel,
        codec,
        &method,
        metadata,
        &stream_id,
        request_rx,
        &event_tx,
    )
    .await;

    // Send end or error event
    match result {
        Ok(()) => {
            let _ = event_tx
                .send(StreamEvent::End {
                    stream_id: stream_id.clone(),
                })
                .await;
        }
        Err(e) => {
            let _ = event_tx
                .send(StreamEvent::Error {
                    stream_id: stream_id.clone(),
                    message: e.to_string(),
                })
                .await;
        }
    }
}

async fn execute_bidi_stream(
    channel: Channel,
    codec: Arc<crate::ffi::CodecBridge>,
    method: &str,
    metadata: HashMap<String, String>,
    stream_id: &str,
    request_rx: mpsc::Receiver<String>,
    event_tx: &mpsc::Sender<StreamEvent>,
) -> Result<()> {
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tower::ServiceExt;

    // Parse method path (format: "/Service/Method" or "Service/Method")
    let method_path = if method.starts_with('/') {
        method.to_string()
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
            http::HeaderValue::from_str(value),
        ) {
            request_builder = request_builder.header(header_name, header_value);
        }
    }

    // Create a streaming body that yields encoded messages from the request channel
    let (body_tx, body_rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(32);

    // Spawn a task to encode and send messages
    let encode_task = tokio::spawn({
        let codec = codec.clone();
        let method = method.to_string();
        async move {
            let mut request_rx = request_rx;
            while let Some(json_payload) = request_rx.recv().await {
                match codec.encode_request(&method, &json_payload) {
                    Ok(wire_data) => {
                        if body_tx.send(bytes::Bytes::from(wire_data)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to encode request: {}", e);
                        break;
                    }
                }
            }
            // Channel closed, body_rx will end when body_tx is dropped
        }
    });

    // Create a custom body stream
    struct StreamingBody {
        rx: tokio::sync::mpsc::Receiver<bytes::Bytes>,
    }

    impl http_body::Body for StreamingBody {
        type Data = bytes::Bytes;
        type Error = std::convert::Infallible;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Option<std::result::Result<http_body::Frame<Self::Data>, Self::Error>>> {
            match self.rx.poll_recv(cx) {
                Poll::Ready(Some(data)) => {
                    Poll::Ready(Some(Ok(http_body::Frame::data(data))))
                }
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    let body = StreamingBody { rx: body_rx };
    let boxed_body = body.map_err(|e: std::convert::Infallible| match e {}).boxed_unsync();

    let mut metadata_keys: Vec<&str> = metadata.keys().map(String::as_str).collect();
    metadata_keys.sort_unstable();
    let request_context = format!(
        "stream_id={}, method_path={}, metadata_keys={:?}",
        stream_id,
        method_path,
        metadata_keys
    );

    let request = request_builder
        .body(boxed_body)
        .map_err(|e| {
            let error_report = format_error_with_chain(&e);
            log::error!(
                "gRPC bidi stream request build failed: diagnostics={}, context={}",
                error_report,
                request_context
            );
            AppError::GrpcStreamFailed(format!(
                "Failed to build request: {}; context={}",
                error_report, request_context
            ))
        })?;

    // Perform the streaming call using tower service
    let response = channel
        .oneshot(request)
        .await
        .map_err(|e| {
            let error_report = format_error_with_chain(&e);
            log::error!(
                "gRPC bidi stream request failed: diagnostics={}, context={}",
                error_report,
                request_context
            );
            AppError::GrpcStreamFailed(format!(
                "Request failed: {}; context={}",
                error_report, request_context
            ))
        })?;

    // Extract response headers
    let response_headers = response.headers().clone();
    let response_metadata = crate::grpc::metadata::headers_to_metadata(&response_headers);

    // Send metadata event
    if !response_metadata.is_empty() {
        let _ = event_tx
            .send(StreamEvent::Metadata {
                stream_id: stream_id.to_string(),
                metadata: response_metadata,
            })
            .await;
    }

    // Get response body stream
    let body = response.into_body();

    // Stream responses using http_body::Body trait concurrently with the encode task
    let stream_result = stream_body_bidi(body, method, codec.as_ref(), stream_id, event_tx).await;

    // Wait for encode task to complete
    let _ = encode_task.await;

    stream_result
}

/// Stream body chunks and decode them for bidirectional streaming
async fn stream_body_bidi<B>(
    mut body: B,
    method: &str,
    codec: &crate::ffi::CodecBridge,
    stream_id: &str,
    event_tx: &mpsc::Sender<StreamEvent>,
) -> Result<()>
where
    B: http_body::Body + Unpin,
    B::Error: std::fmt::Display,
    B::Data: bytes::Buf,
{
    // Stream responses
    while let Some(frame_result) = body.frame().await {
        let frame = frame_result.map_err(|e| {
            AppError::GrpcStreamFailed(format!("Failed to read response chunk: {}", e))
        })?;

        // Get data from frame if it's a data frame
        let chunk = match frame.data_ref() {
            Some(data) => data.chunk(),
            None => continue,
        };

        if chunk.is_empty() {
            continue;
        }

        // Decode response from wire format to JSON
        match codec.decode_response(method, chunk) {
            Ok(json_payload) => {
                // Parse JSON
                match serde_json::from_str::<serde_json::Value>(&json_payload) {
                    Ok(data) => {
                        let _ = event_tx
                            .send(StreamEvent::Message {
                                stream_id: stream_id.to_string(),
                                data,
                            })
                            .await;
                    }
                    Err(e) => {
                        return Err(AppError::GrpcStreamFailed(format!(
                            "Failed to parse JSON response: {}",
                            e
                        )));
                    }
                }
            }
            Err(e) => {
                return Err(AppError::GrpcStreamFailed(format!(
                    "Failed to decode response: {}",
                    e
                )));
            }
        }
    }

    Ok(())
}

/// Parse a method path in format "service/method" or "service.method"
fn parse_method_path(method: &str) -> Result<(String, String)> {
    // 统一先去掉前导斜杠，避免 "/service/method" 被误判为 service 为空。
    let normalized = method.trim_start_matches('/');

    // Try "service/method" format first
    if let Some(pos) = normalized.find('/') {
        let service = normalized[..pos].to_string();
        let method_name = normalized[pos + 1..].to_string();
        if !service.is_empty() && !method_name.is_empty() {
            return Ok((service, method_name));
        }
    }

    // Try "service.method" format
    if let Some(pos) = normalized.rfind('.') {
        let service = normalized[..pos].to_string();
        let method_name = normalized[pos + 1..].to_string();
        if !service.is_empty() && !method_name.is_empty() {
            return Ok((service, method_name));
        }
    }

    Err(AppError::GrpcMethodNotFound(format!(
        "Invalid method format: {}. Expected 'service/method' or 'service.method'",
        method
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_method_path() {
        // Test service/method format
        let (svc, method) = parse_method_path("my.service/MyMethod").unwrap();
        assert_eq!(svc, "my.service");
        assert_eq!(method, "MyMethod");

        // Test service.method format
        let (svc, method) = parse_method_path("my.service.MyMethod").unwrap();
        assert_eq!(svc, "my.service");
        assert_eq!(method, "MyMethod");

        // Test /service/method format
        let (svc, method) = parse_method_path("/my.service/MyMethod").unwrap();
        assert_eq!(svc, "my.service");
        assert_eq!(method, "MyMethod");

        // Test invalid format
        assert!(parse_method_path("invalid").is_err());
    }

    #[test]
    fn test_stream_type() {
        assert!(StreamType::ServerStreaming.has_server_stream());
        assert!(!StreamType::ServerStreaming.has_client_stream());

        assert!(!StreamType::ClientStreaming.has_server_stream());
        assert!(StreamType::ClientStreaming.has_client_stream());

        assert!(StreamType::Bidirectional.has_server_stream());
        assert!(StreamType::Bidirectional.has_client_stream());
    }

    #[tokio::test]
    async fn test_stream_manager_basic() {
        let manager = StreamManager::new();

        // Initially no channel
        assert!(!manager.has_channel().await);
        // 初始化阶段不应存在共享 codec，避免误判已连接状态。
        assert!(!manager.has_codec().await);

        // Initially no streams
        assert_eq!(manager.active_stream_count().await, 0);

        // Non-existent stream check
        assert!(!manager.stream_exists("non-existent").await);
    }
}
