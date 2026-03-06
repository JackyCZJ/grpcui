//! gRPC Client Module
//!
//! This module provides a Rust-based gRPC client implementation that:
//! - Uses the FFI bridge (CodecBridge) for protobuf encoding/decoding
//! - Uses tonic for HTTP/2 transport
//! - Supports unary calls with metadata, timeouts, and proper error handling

pub mod client;
pub mod metadata;
pub mod transport;
pub mod streaming;

/// format_error_with_chain 将错误链与 Debug 视图拼接为可诊断字符串。
///
/// 设计目标：
/// 1) 保留 Display 语义，便于终端和前端直接阅读；
/// 2) 递归展开 source 链，避免只看到最外层 "transport error"；
/// 3) 附带 Debug 结构，便于定位底层 hyper/h2/IO 细节。
pub(crate) fn format_error_with_chain<E>(error: &E) -> String
where
    E: std::error::Error + std::fmt::Debug,
{
    let mut chain = vec![error.to_string()];
    let mut source = error.source();

    while let Some(cause) = source {
        chain.push(cause.to_string());
        source = cause.source();
    }

    format!("chain=[{}]; debug={:?}", chain.join(" <- "), error)
}

// Re-export main types for convenience
pub use client::{GrpcClient, TlsConfig};
