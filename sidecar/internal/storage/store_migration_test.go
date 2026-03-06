package storage

import (
	"database/sql"
	"path/filepath"
	"testing"

	_ "github.com/mattn/go-sqlite3"
)

// createLegacySchema 创建一个旧版数据库结构，
// 该结构缺少项目化改造新增的 project_id 字段，用于验证迁移逻辑兼容性。
func createLegacySchema(t *testing.T, dbPath string) {
	t.Helper()

	db, err := sql.Open("sqlite3", dbPath)
	if err != nil {
		t.Fatalf("failed to open legacy db: %v", err)
	}
	defer func() {
		_ = db.Close()
	}()

	schema := `
	CREATE TABLE environments (
		id TEXT PRIMARY KEY,
		name TEXT NOT NULL,
		base_url TEXT NOT NULL,
		variables TEXT,
		headers TEXT,
		tls_config TEXT,
		created_at DATETIME,
		updated_at DATETIME
	);

	CREATE TABLE collections (
		id TEXT PRIMARY KEY,
		name TEXT NOT NULL,
		data TEXT NOT NULL,
		created_at DATETIME,
		updated_at DATETIME
	);

	CREATE TABLE history (
		id TEXT PRIMARY KEY,
		timestamp INTEGER NOT NULL,
		service TEXT NOT NULL,
		method TEXT NOT NULL,
		address TEXT NOT NULL,
		status TEXT NOT NULL,
		duration INTEGER NOT NULL,
		request_snapshot TEXT NOT NULL,
		created_at DATETIME
	);
	`

	if _, err := db.Exec(schema); err != nil {
		t.Fatalf("failed to create legacy schema: %v", err)
	}
}

// TestNewSQLiteStoreMigratesLegacyProjectColumns 验证旧库在启动时会自动补齐 project_id 相关字段，
// 避免因为先建索引后补列导致 sidecar 启动失败。
func TestNewSQLiteStoreMigratesLegacyProjectColumns(t *testing.T) {
	dbPath := filepath.Join(t.TempDir(), "legacy.db")
	createLegacySchema(t, dbPath)

	store, err := NewSQLiteStore(dbPath)
	if err != nil {
		t.Fatalf("expected migration success for legacy db, got error: %v", err)
	}
	defer func() {
		_ = store.Close()
	}()

	hasHistoryProjectID, err := store.hasColumn("history", "project_id")
	if err != nil {
		t.Fatalf("failed to inspect history columns: %v", err)
	}
	if !hasHistoryProjectID {
		t.Fatalf("expected history.project_id to exist after migration")
	}

	hasCollectionsProjectID, err := store.hasColumn("collections", "project_id")
	if err != nil {
		t.Fatalf("failed to inspect collections columns: %v", err)
	}
	if !hasCollectionsProjectID {
		t.Fatalf("expected collections.project_id to exist after migration")
	}
}
