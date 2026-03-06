-- Add TLS configuration table

CREATE TABLE IF NOT EXISTS tls_configs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    enabled BOOLEAN DEFAULT 0,
    ca_file TEXT,
    cert_file TEXT,
    key_file TEXT,
    server_name TEXT,
    insecure BOOLEAN DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Index for TLS config lookups
CREATE INDEX IF NOT EXISTS idx_tls_configs_name ON tls_configs(name);

-- Migration: Move existing TLS config from environments to new structure
-- This is a placeholder for data migration if needed
-- UPDATE environments SET tls_config = '{}' WHERE tls_config IS NULL;
