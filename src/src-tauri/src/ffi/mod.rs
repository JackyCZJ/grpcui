//! FFI module for Go codec bridge
//!
//! This module provides Rust bindings to the Go FFI library for
//! protobuf message encoding and decoding.

pub mod bridge;
pub mod error;

pub use bridge::{CodecBridge, ReflectionTlsConfig};
pub use error::FfiError;

/// FFIBridge 是 CodecBridge 的别名，用于向后兼容 main.rs
pub type FFIBridge = CodecBridge;
