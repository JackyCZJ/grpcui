-- Add performance indexes

-- History indexes
CREATE INDEX IF NOT EXISTS idx_history_service ON history(service);
CREATE INDEX IF NOT EXISTS idx_history_method ON history(method);
CREATE INDEX IF NOT EXISTS idx_history_status ON history(status);

-- Environment indexes
CREATE INDEX IF NOT EXISTS idx_environments_name ON environments(name);

-- Collection indexes
CREATE INDEX IF NOT EXISTS idx_collections_name ON collections(name);

-- Composite indexes for common queries
CREATE INDEX IF NOT EXISTS idx_history_service_method ON history(service, method);
CREATE INDEX IF NOT EXISTS idx_history_timestamp_status ON history(timestamp, status);
