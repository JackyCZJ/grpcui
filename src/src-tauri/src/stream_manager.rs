#![allow(dead_code)]
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::AbortHandle;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::error::{AppError, Result};
use crate::ffi::CodecBridge;
use crate::grpc::streaming::{StreamManager as GrpcStreamManager, StreamType as GrpcStreamType, StreamEvent as GrpcStreamEvent};

/// Frontend stream event types sent to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FrontendStreamEvent {
    /// Message received from server
    #[serde(rename = "message")]
    Message { data: serde_json::Value },
    /// Server stream ended
    #[serde(rename = "end")]
    End,
    /// Error occurred
    #[serde(rename = "error")]
    Error { message: String },
    /// Stream metadata received
    #[serde(rename = "metadata")]
    Metadata { metadata: HashMap<String, String> },
}

/// Handle to a managed stream
#[allow(dead_code)]
struct StreamHandle {
    sender: mpsc::Sender<Vec<u8>>,
    abort_handle: AbortHandle,
}

/// Manages active gRPC streams
///
/// This struct provides both the legacy HTTP sidecar-based streaming
/// and the new native gRPC streaming capabilities.
pub struct StreamManager {
    /// Legacy HTTP sidecar streams
    streams: Arc<RwLock<HashMap<String, StreamHandle>>>,
    /// Native gRPC stream manager
    grpc_manager: GrpcStreamManager,
    /// Event channel for native streams
    event_tx: mpsc::Sender<FrontendStreamEvent>,
    /// Event receiver - stored to keep the channel open
    #[allow(dead_code)]
    event_rx: Arc<RwLock<mpsc::Receiver<FrontendStreamEvent>>>,
}

impl StreamManager {
    /// Create a new StreamManager
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(1000);
        let grpc_manager = GrpcStreamManager::new();

        Self {
            streams: Arc::new(RwLock::new(HashMap::new())),
            grpc_manager,
            event_tx,
            event_rx: Arc::new(RwLock::new(event_rx)),
        }
    }

    /// Get a reference to the native gRPC stream manager
    pub fn grpc_manager(&self) -> &GrpcStreamManager {
        &self.grpc_manager
    }

    /// Register a new stream (legacy HTTP sidecar mode)
    pub async fn register_stream(
        &self,
        stream_id: String,
        sender: mpsc::Sender<Vec<u8>>,
        abort_handle: AbortHandle,
    ) {
        let mut streams = self.streams.write().await;
        streams.insert(
            stream_id,
            StreamHandle {
                sender,
                abort_handle,
            },
        );
    }

    /// Send a message to a stream (legacy HTTP sidecar mode)
    #[allow(dead_code)]
    pub async fn send_message(&self, stream_id: &str, message: Vec<u8>) -> Result<()> {
        let streams = self.streams.read().await;
        let handle = streams
            .get(stream_id)
            .ok_or_else(|| AppError::GrpcStreamFailed(format!("Stream {} not found", stream_id)))?;

        handle
            .sender
            .send(message)
            .await
            .map_err(|_| AppError::GrpcStreamFailed("Failed to send message to stream".to_string()))?;

        Ok(())
    }

    /// Close a stream and clean up resources (legacy HTTP sidecar mode)
    pub async fn close_stream(&self, stream_id: &str) -> Result<()> {
        let mut streams = self.streams.write().await;
        let handle = streams
            .remove(stream_id)
            .ok_or_else(|| AppError::GrpcStreamFailed(format!("Stream {} not found", stream_id)))?;

        // Abort the stream task
        handle.abort_handle.abort();

        Ok(())
    }

    /// Emit a stream event to the frontend
    pub fn emit_event(app_handle: &AppHandle, stream_id: &str, event: FrontendStreamEvent) {
        let event_name = format!("grpc:stream:{}", stream_id);
        if let Err(e) = app_handle.emit(&event_name, event) {
            log::error!("Failed to emit stream event: {}", e);
        }
    }

    /// Check if a stream exists (legacy mode)
    pub async fn stream_exists(&self, stream_id: &str) -> bool {
        let streams = self.streams.read().await;
        streams.contains_key(stream_id)
    }

    /// Start a native gRPC stream
    ///
    /// # Arguments
    /// * `method` - The gRPC method path (e.g., "package.Service/Method")
    /// * `stream_type` - Type of streaming (server, client, bidi)
    /// * `initial_request` - Initial request message for server streaming
    /// * `metadata` - gRPC metadata to send with the request
    /// * `app_handle` - Tauri app handle for emitting events
    pub async fn start_native_stream(
        &self,
        method: &str,
        stream_type: GrpcStreamType,
        initial_request: Option<String>,
        metadata: HashMap<String, String>,
        app_handle: AppHandle,
    ) -> Result<String> {
        // Check if we have a gRPC channel
        if !self.grpc_manager.has_channel().await {
            return Err(AppError::GrpcConnectionFailed(
                "No gRPC connection available. Call grpc_connect first.".to_string(),
            ));
        }

        // Create a channel for receiving events from the stream
        let (tx, mut rx) = mpsc::channel::<GrpcStreamEvent>(100);

        // Start the stream
        let handle = self
            .grpc_manager
            .start_stream(method, stream_type, initial_request, metadata, tx)
            .await?;

        let stream_id = handle.id.clone();

        // Spawn a task to forward events to the frontend
        let stream_id_clone = stream_id.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let frontend_event = match event {
                    GrpcStreamEvent::Message { stream_id: _, data } => FrontendStreamEvent::Message { data },
                    GrpcStreamEvent::Metadata { stream_id: _, metadata } => {
                        FrontendStreamEvent::Metadata { metadata }
                    }
                    GrpcStreamEvent::Error {
                        stream_id: _,
                        message,
                    } => FrontendStreamEvent::Error { message },
                    GrpcStreamEvent::End { stream_id: _ } => FrontendStreamEvent::End,
                };

                Self::emit_event(&app_handle, &stream_id_clone, frontend_event);
            }
        });

        Ok(stream_id)
    }

    /// Send a message to a native gRPC stream (client/bidi only)
    pub async fn send_native_stream_message(
        &self,
        stream_id: &str,
        json_payload: &str,
    ) -> Result<()> {
        self.grpc_manager
            .send_message(stream_id, json_payload)
            .await
    }

    /// End a native gRPC stream (signal half-close for client/bidi)
    pub async fn end_native_stream(&self, stream_id: &str) -> Result<()> {
        self.grpc_manager.end_stream(stream_id).await
    }

    /// Cancel a native gRPC stream
    pub async fn cancel_native_stream(&self, stream_id: &str) -> Result<()> {
        self.grpc_manager.cancel_stream(stream_id).await
    }

    /// Set the gRPC channel for native streaming
    pub async fn set_grpc_channel(&self, channel: tonic::transport::Channel) {
        self.grpc_manager.set_channel(channel).await;
    }

    /// Set shared FFI codec for native streaming
    ///
    /// 复用同一份 codec 可确保 stream encode/decode 与 grpc_connect 的描述集一致。
    pub async fn set_grpc_codec(&self, codec: Arc<CodecBridge>) {
        self.grpc_manager.set_codec(codec).await;
    }

    /// Clear the gRPC channel
    pub async fn clear_grpc_channel(&self) {
        self.grpc_manager.clear_channel().await;
    }

    /// Clear shared FFI codec for native streaming
    pub async fn clear_grpc_codec(&self) {
        self.grpc_manager.clear_codec().await;
    }

    /// Get the number of active native streams
    pub async fn active_native_stream_count(&self) -> usize {
        self.grpc_manager.active_stream_count().await
    }
}

impl Default for StreamManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stream_manager_creation() {
        let manager = StreamManager::new();
        assert!(!manager.stream_exists("test").await);
        assert_eq!(manager.active_native_stream_count().await, 0);
    }

    #[test]
    fn test_stream_event_serialization() {
        let event = FrontendStreamEvent::Message {
            data: serde_json::json!({"key": "value"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("message"));
        assert!(json.contains("key"));

        let event = FrontendStreamEvent::End;
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("end"));

        let event = FrontendStreamEvent::Error {
            message: "test error".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("test error"));
    }
}
