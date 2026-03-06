-- Initial schema for gRPC UI sidecar

-- Environments table
CREATE TABLE IF NOT EXISTS environments (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    variables TEXT,
    headers TEXT,
    tls_config TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Collections table
CREATE TABLE IF NOT EXISTS collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    data TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- History table
CREATE TABLE IF NOT EXISTS history (
    id TEXT PRIMARY KEY,
    timestamp INTEGER NOT NULL,
    service TEXT NOT NULL,
    method TEXT NOT NULL,
    address TEXT NOT NULL,
    status TEXT NOT NULL,
    duration INTEGER NOT NULL,
    request_snapshot TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Initial indexes
CREATE INDEX IF NOT EXISTS idx_history_timestamp ON history(timestamp DESC);
