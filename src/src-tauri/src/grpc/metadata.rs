//! gRPC Metadata handling
//!
//! This module provides utilities for handling gRPC metadata (headers/trailers).

#![allow(dead_code)]
use std::collections::HashMap;
use http::HeaderMap;

/// Convert a HashMap to HTTP HeaderMap for gRPC metadata
pub fn metadata_to_headers(metadata: &HashMap<String, String>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    
    for (key, value) in metadata {
        // gRPC metadata keys should be lowercase
        let header_name = format!("x-{}-bin", key.to_lowercase());
        if let Ok(name) = http::HeaderName::from_bytes(header_name.as_bytes()) {
            if let Ok(val) = http::HeaderValue::from_str(value) {
                headers.insert(name, val);
            }
        }
    }
    
    headers
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

/// Parse gRPC status from headers
pub fn parse_grpc_status(headers: &HeaderMap) -> Option<i32> {
    headers
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
}

/// Parse gRPC status message from headers
pub fn parse_grpc_message(headers: &HeaderMap) -> Option<String> {
    headers
        .get("grpc-message")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Build gRPC metadata headers from HashMap
pub fn build_metadata_headers(metadata: &HashMap<String, String>) -> Vec<(http::HeaderName, http::HeaderValue)> {
    let mut headers = Vec::new();
    
    for (key, value) in metadata {
        // Try as-is first
        if let (Ok(name), Ok(val)) = (
            http::HeaderName::from_bytes(key.as_bytes()),
            http::HeaderValue::from_str(value)
        ) {
            headers.push((name, val));
        }
    }
    
    headers
}

/// Metadata builder for constructing gRPC metadata
#[derive(Debug, Default)]
pub struct MetadataBuilder {
    metadata: HashMap<String, String>,
}

impl MetadataBuilder {
    /// Create a new metadata builder
    pub fn new() -> Self {
        Self {
            metadata: HashMap::new(),
        }
    }
    
    /// Add a metadata entry
    pub fn add(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
    
    /// Build the metadata HashMap
    pub fn build(self) -> HashMap<String, String> {
        self.metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_headers_to_metadata() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/grpc".parse().unwrap());
        headers.insert("custom-header", "custom-value".parse().unwrap());
        
        let metadata = headers_to_metadata(&headers);
        
        assert_eq!(metadata.get("content-type"), Some(&"application/grpc".to_string()));
        assert_eq!(metadata.get("custom-header"), Some(&"custom-value".to_string()));
    }
    
    #[test]
    fn test_parse_grpc_status() {
        let mut headers = HeaderMap::new();
        assert_eq!(parse_grpc_status(&headers), None);
        
        headers.insert("grpc-status", "0".parse().unwrap());
        assert_eq!(parse_grpc_status(&headers), Some(0));
        
        headers.insert("grpc-status", "14".parse().unwrap());
        assert_eq!(parse_grpc_status(&headers), Some(14));
    }
    
    #[test]
    fn test_parse_grpc_message() {
        let mut headers = HeaderMap::new();
        assert_eq!(parse_grpc_message(&headers), None);
        
        headers.insert("grpc-message", "Not found".parse().unwrap());
        assert_eq!(parse_grpc_message(&headers), Some("Not found".to_string()));
    }
    
    #[test]
    fn test_metadata_builder() {
        let metadata = MetadataBuilder::new()
            .add("key1", "value1")
            .add("key2", "value2")
            .build();
        
        assert_eq!(metadata.get("key1"), Some(&"value1".to_string()));
        assert_eq!(metadata.get("key2"), Some(&"value2".to_string()));
    }
}
