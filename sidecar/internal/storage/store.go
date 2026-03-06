package storage

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"strings"
	"time"

	_ "github.com/mattn/go-sqlite3"
)

type Store struct {
	db *sql.DB
}

func NewSQLiteStore(dbPath string) (*Store, error) {
	dir := filepath.Dir(dbPath)
	if dir != "" && dir != "." {
		if err := os.MkdirAll(dir, 0755); err != nil {
			return nil, fmt.Errorf("failed to create directory: %w", err)
		}
	}

	db, err := sql.Open("sqlite3", dbPath+"?_journal_mode=WAL&_busy_timeout=5000&_foreign_keys=on")
	if err != nil {
		return nil, fmt.Errorf("failed to open database: %w", err)
	}

	db.SetMaxOpenConns(25)
	db.SetMaxIdleConns(10)
	db.SetConnMaxLifetime(5 * time.Minute)

	store := &Store{db: db}
	if err := store.migrate(); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("failed to migrate: %w", err)
	}

	return store, nil
}

func (s *Store) migrate() error {
	schema := `
	-- Projects table
	CREATE TABLE IF NOT EXISTS projects (
		id TEXT PRIMARY KEY,
		name TEXT NOT NULL,
		description TEXT,
		default_environment_id TEXT,
		proto_files TEXT,
		created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
		updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
	);

	CREATE INDEX IF NOT EXISTS idx_projects_name ON projects(name);

	-- Environments table (with project_id for backward compatibility)
	CREATE TABLE IF NOT EXISTS environments (
		id TEXT PRIMARY KEY,
		project_id TEXT,
		name TEXT NOT NULL,
		base_url TEXT NOT NULL,
		variables TEXT,
		headers TEXT,
		tls_config TEXT,
		is_default BOOLEAN DEFAULT FALSE,
		created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
		updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
		FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE SET NULL
	);

	CREATE INDEX IF NOT EXISTS idx_environments_name ON environments(name);

	-- Project-Environments association table (for many-to-many relationship)
	CREATE TABLE IF NOT EXISTS project_environments (
		project_id TEXT NOT NULL,
		environment_id TEXT NOT NULL,
		is_default BOOLEAN DEFAULT FALSE,
		PRIMARY KEY (project_id, environment_id),
		FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
		FOREIGN KEY (environment_id) REFERENCES environments(id) ON DELETE CASCADE
	);

	CREATE INDEX IF NOT EXISTS idx_proj_env_project ON project_environments(project_id);
	CREATE INDEX IF NOT EXISTS idx_proj_env_env ON project_environments(environment_id);

	-- Collections table (with project_id)
	CREATE TABLE IF NOT EXISTS collections (
		id TEXT PRIMARY KEY,
		project_id TEXT,
		name TEXT NOT NULL,
		data TEXT NOT NULL,
		created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
		updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
		FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE SET NULL
	);

	CREATE INDEX IF NOT EXISTS idx_collections_name ON collections(name);

	-- History table (with project_id)
	CREATE TABLE IF NOT EXISTS history (
		id TEXT PRIMARY KEY,
		project_id TEXT,
		timestamp INTEGER NOT NULL,
		service TEXT NOT NULL,
		method TEXT NOT NULL,
		address TEXT NOT NULL,
		status TEXT NOT NULL,
		duration INTEGER NOT NULL,
		request_snapshot TEXT NOT NULL,
		created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
		FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE SET NULL
	);

	CREATE INDEX IF NOT EXISTS idx_history_timestamp ON history(timestamp DESC);
	CREATE INDEX IF NOT EXISTS idx_history_service ON history(service);
	CREATE INDEX IF NOT EXISTS idx_history_method ON history(method);
	CREATE INDEX IF NOT EXISTS idx_history_status ON history(status);
	`

	_, err := s.db.Exec(schema)
	if err != nil {
		return fmt.Errorf("failed to create schema: %w", err)
	}

	if err := s.ensureSchemaColumns(); err != nil {
		return fmt.Errorf("failed to ensure schema columns: %w", err)
	}

	if err := s.ensureProjectScopedIndexes(); err != nil {
		return fmt.Errorf("failed to ensure project scoped indexes: %w", err)
	}

	// Migrate existing data: create a default project for orphaned environments
	if err := s.migrateExistingData(); err != nil {
		return fmt.Errorf("failed to migrate existing data: %w", err)
	}

	return nil
}

// ensureProjectScopedIndexes 在补齐 project_id 字段后创建相关索引，
// 避免老库在字段缺失阶段提前建索引导致启动失败。
func (s *Store) ensureProjectScopedIndexes() error {
	statements := []string{
		`CREATE INDEX IF NOT EXISTS idx_environments_project ON environments(project_id)`,
		`CREATE INDEX IF NOT EXISTS idx_collections_project ON collections(project_id)`,
		`CREATE INDEX IF NOT EXISTS idx_history_project ON history(project_id)`,
	}

	for _, statement := range statements {
		if _, err := s.db.Exec(statement); err != nil {
			return err
		}
	}

	return nil
}

// ensureSchemaColumns 确保历史数据库具备项目化改造所需字段。
// 由于 SQLite 的 CREATE TABLE IF NOT EXISTS 不会自动补齐旧表列，
// 这里通过 PRAGMA table_info 检查并按需执行 ALTER TABLE。
func (s *Store) ensureSchemaColumns() error {
	columns := []struct {
		table      string
		name       string
		definition string
	}{
		{table: "environments", name: "project_id", definition: "project_id TEXT"},
		{table: "environments", name: "is_default", definition: "is_default BOOLEAN DEFAULT FALSE"},
		{table: "collections", name: "project_id", definition: "project_id TEXT"},
		{table: "history", name: "project_id", definition: "project_id TEXT"},
	}

	for _, col := range columns {
		hasColumn, err := s.hasColumn(col.table, col.name)
		if err != nil {
			return err
		}
		if hasColumn {
			continue
		}

		query := fmt.Sprintf("ALTER TABLE %s ADD COLUMN %s", col.table, col.definition)
		if _, err := s.db.Exec(query); err != nil {
			return fmt.Errorf("failed to add %s.%s: %w", col.table, col.name, err)
		}
	}

	return nil
}

// hasColumn 通过 PRAGMA table_info 判断表中是否存在指定列。
func (s *Store) hasColumn(table, column string) (bool, error) {
	query := fmt.Sprintf("PRAGMA table_info(%s)", table)
	rows, err := s.db.Query(query)
	if err != nil {
		return false, err
	}
	defer func() {
		_ = rows.Close()
	}()

	for rows.Next() {
		var cid int
		var name string
		var dataType string
		var notNull int
		var defaultValue sql.NullString
		var pk int
		if err := rows.Scan(&cid, &name, &dataType, &notNull, &defaultValue, &pk); err != nil {
			return false, err
		}
		if name == column {
			return true, nil
		}
	}

	if err := rows.Err(); err != nil {
		return false, err
	}

	return false, nil
}

// migrateExistingData handles backward compatibility by creating a default project
// for existing environments that don't have a project_id
func (s *Store) migrateExistingData() error {
	// Check if there are any environments without project_id
	var count int
	err := s.db.QueryRow(`SELECT COUNT(*) FROM environments WHERE project_id IS NULL`).Scan(&count)
	if err != nil {
		return err
	}

	if count == 0 {
		return nil // No migration needed
	}

	// Create a default project for orphaned environments
	defaultProjectID := "default-project"
	now := time.Now()

	_, err = s.db.Exec(
		`INSERT OR IGNORE INTO projects (id, name, description, proto_files, created_at, updated_at)
		 VALUES (?, ?, ?, ?, ?, ?)`,
		defaultProjectID, "Default Project", "Auto-created project for existing data", "[]", now, now,
	)
	if err != nil {
		return err
	}

	// Assign orphaned environments to the default project
	_, err = s.db.Exec(
		`UPDATE environments SET project_id = ? WHERE project_id IS NULL`,
		defaultProjectID,
	)
	if err != nil {
		return err
	}

	// Assign orphaned collections to the default project
	_, err = s.db.Exec(
		`UPDATE collections SET project_id = ? WHERE project_id IS NULL`,
		defaultProjectID,
	)
	if err != nil {
		return err
	}

	// Assign orphaned history entries to the default project
	_, err = s.db.Exec(
		`UPDATE history SET project_id = ? WHERE project_id IS NULL`,
		defaultProjectID,
	)

	return err
}

func (s *Store) Close() error {
	return s.db.Close()
}

func (s *Store) DB() *sql.DB {
	return s.db
}

// Transaction support
func (s *Store) BeginTx(ctx context.Context) (*sql.Tx, error) {
	return s.db.BeginTx(ctx, nil)
}

func (s *Store) CommitTx(tx *sql.Tx) error {
	return tx.Commit()
}

func (s *Store) RollbackTx(tx *sql.Tx) error {
	return tx.Rollback()
}

// Environment operations
func (s *Store) SaveEnvironment(env *Environment) error {
	variables, err := json.Marshal(env.Variables)
	if err != nil {
		return fmt.Errorf("failed to marshal variables: %w", err)
	}

	headers, err := json.Marshal(env.Headers)
	if err != nil {
		return fmt.Errorf("failed to marshal headers: %w", err)
	}

	tlsConfig, err := json.Marshal(env.TLSConfig)
	if err != nil {
		return fmt.Errorf("failed to marshal tls config: %w", err)
	}

	createdAt := env.CreatedAt
	if createdAt.IsZero() {
		createdAt = time.Now()
	}
	updatedAt := time.Now()

	_, err = s.db.Exec(
		`INSERT INTO environments (id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
		 ON CONFLICT(id) DO UPDATE SET
		 project_id=excluded.project_id,
		 name=excluded.name,
		 base_url=excluded.base_url,
		 variables=excluded.variables,
		 headers=excluded.headers,
		 tls_config=excluded.tls_config,
		 is_default=excluded.is_default,
		 updated_at=excluded.updated_at`,
		env.ID,
		env.ProjectID,
		env.Name,
		env.BaseURL,
		string(variables),
		string(headers),
		string(tlsConfig),
		env.IsDefault,
		createdAt,
		updatedAt,
	)
	if err != nil {
		return err
	}

	if env.ProjectID != "" {
		_, err = s.db.Exec(
			`INSERT INTO project_environments (project_id, environment_id, is_default)
			 VALUES (?, ?, ?)
			 ON CONFLICT(project_id, environment_id) DO UPDATE SET is_default=excluded.is_default`,
			env.ProjectID,
			env.ID,
			env.IsDefault,
		)
		if err != nil {
			return err
		}

		if env.IsDefault {
			if err := s.SetDefaultEnvironment(env.ProjectID, env.ID); err != nil {
				return err
			}
		}
	}

	return nil
}

func (s *Store) GetEnvironments() ([]Environment, error) {
	rows, err := s.db.Query(`
		SELECT id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at
		FROM environments
		ORDER BY name
	`)
	if err != nil {
		return nil, err
	}
	defer func() {
		_ = rows.Close()
	}()

	var envs []Environment
	for rows.Next() {
		var env Environment
		var projectID sql.NullString
		var variables sql.NullString
		var headers sql.NullString
		var tlsConfig sql.NullString
		err := rows.Scan(
			&env.ID,
			&projectID,
			&env.Name,
			&env.BaseURL,
			&variables,
			&headers,
			&tlsConfig,
			&env.IsDefault,
			&env.CreatedAt,
			&env.UpdatedAt,
		)
		if err != nil {
			return nil, err
		}
		if projectID.Valid {
			env.ProjectID = projectID.String
		}
		if variables.Valid && variables.String != "" {
			if err := json.Unmarshal([]byte(variables.String), &env.Variables); err != nil {
				log.Printf("Warning: failed to unmarshal variables for env %s: %v", env.ID, err)
				env.Variables = make(map[string]string)
			}
		} else {
			env.Variables = make(map[string]string)
		}
		if headers.Valid && headers.String != "" {
			if err := json.Unmarshal([]byte(headers.String), &env.Headers); err != nil {
				log.Printf("Warning: failed to unmarshal headers for env %s: %v", env.ID, err)
				env.Headers = make(map[string]string)
			}
		} else {
			env.Headers = make(map[string]string)
		}
		if tlsConfig.Valid && tlsConfig.String != "" {
			if err := json.Unmarshal([]byte(tlsConfig.String), &env.TLSConfig); err != nil {
				log.Printf("Warning: failed to unmarshal tls_config for env %s: %v", env.ID, err)
				env.TLSConfig = nil
			}
		} else {
			env.TLSConfig = nil
		}
		envs = append(envs, env)
	}
	return envs, rows.Err()
}

func (s *Store) GetEnvironment(id string) (*Environment, error) {
	var env Environment
	var projectID sql.NullString
	var variables sql.NullString
	var headers sql.NullString
	var tlsConfig sql.NullString
	err := s.db.QueryRow(
		`SELECT id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at FROM environments WHERE id = ?`,
		id,
	).Scan(
		&env.ID,
		&projectID,
		&env.Name,
		&env.BaseURL,
		&variables,
		&headers,
		&tlsConfig,
		&env.IsDefault,
		&env.CreatedAt,
		&env.UpdatedAt,
	)
	if err != nil {
		return nil, err
	}
	if projectID.Valid {
		env.ProjectID = projectID.String
	}
	if variables.Valid && variables.String != "" {
		if err := json.Unmarshal([]byte(variables.String), &env.Variables); err != nil {
			log.Printf("Warning: failed to unmarshal variables for env %s: %v", env.ID, err)
			env.Variables = make(map[string]string)
		}
	} else {
		env.Variables = make(map[string]string)
	}
	if headers.Valid && headers.String != "" {
		if err := json.Unmarshal([]byte(headers.String), &env.Headers); err != nil {
			log.Printf("Warning: failed to unmarshal headers for env %s: %v", env.ID, err)
			env.Headers = make(map[string]string)
		}
	} else {
		env.Headers = make(map[string]string)
	}
	if tlsConfig.Valid && tlsConfig.String != "" {
		if err := json.Unmarshal([]byte(tlsConfig.String), &env.TLSConfig); err != nil {
			log.Printf("Warning: failed to unmarshal tls_config for env %s: %v", env.ID, err)
			env.TLSConfig = nil
		}
	} else {
		env.TLSConfig = nil
	}
	return &env, nil
}

func (s *Store) DeleteEnvironment(id string) error {
	_, err := s.db.Exec(`DELETE FROM environments WHERE id = ?`, id)
	return err
}

func (s *Store) SearchEnvironments(query string) ([]Environment, error) {
	searchPattern := "%" + strings.ToLower(query) + "%"
	rows, err := s.db.Query(
		`SELECT id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at FROM environments
		 WHERE LOWER(name) LIKE ? OR LOWER(base_url) LIKE ? ORDER BY name`,
		searchPattern, searchPattern,
	)
	if err != nil {
		return nil, err
	}
	defer func() {
		_ = rows.Close()
	}()

	var envs []Environment
	for rows.Next() {
		var env Environment
		var projectID sql.NullString
		var variables sql.NullString
		var headers sql.NullString
		var tlsConfig sql.NullString
		err := rows.Scan(
			&env.ID,
			&projectID,
			&env.Name,
			&env.BaseURL,
			&variables,
			&headers,
			&tlsConfig,
			&env.IsDefault,
			&env.CreatedAt,
			&env.UpdatedAt,
		)
		if err != nil {
			return nil, err
		}
		if projectID.Valid {
			env.ProjectID = projectID.String
		}
		if variables.Valid && variables.String != "" {
			if err := json.Unmarshal([]byte(variables.String), &env.Variables); err != nil {
				log.Printf("Warning: failed to unmarshal variables for env %s: %v", env.ID, err)
				env.Variables = make(map[string]string)
			}
		} else {
			env.Variables = make(map[string]string)
		}
		if headers.Valid && headers.String != "" {
			if err := json.Unmarshal([]byte(headers.String), &env.Headers); err != nil {
				log.Printf("Warning: failed to unmarshal headers for env %s: %v", env.ID, err)
				env.Headers = make(map[string]string)
			}
		} else {
			env.Headers = make(map[string]string)
		}
		if tlsConfig.Valid && tlsConfig.String != "" {
			if err := json.Unmarshal([]byte(tlsConfig.String), &env.TLSConfig); err != nil {
				log.Printf("Warning: failed to unmarshal tls_config for env %s: %v", env.ID, err)
				env.TLSConfig = nil
			}
		} else {
			env.TLSConfig = nil
		}
		envs = append(envs, env)
	}
	return envs, rows.Err()
}

// Collection operations
func (s *Store) SaveCollection(col *Collection) error {
	data, err := json.Marshal(col)
	if err != nil {
		return err
	}

	createdAt := col.CreatedAt
	if createdAt.IsZero() {
		createdAt = time.Now()
	}
	updatedAt := time.Now()

	_, err = s.db.Exec(
		`INSERT INTO collections (id, project_id, name, data, created_at, updated_at)
		 VALUES (?, ?, ?, ?, ?, ?)
		 ON CONFLICT(id) DO UPDATE SET
		 project_id=excluded.project_id,
		 name=excluded.name,
		 data=excluded.data,
		 updated_at=excluded.updated_at`,
		col.ID,
		col.ProjectID,
		col.Name,
		string(data),
		createdAt,
		updatedAt,
	)
	return err
}

func (s *Store) GetCollections() ([]Collection, error) {
	rows, err := s.db.Query(`SELECT project_id, data FROM collections ORDER BY name`)
	if err != nil {
		return nil, err
	}
	defer func() {
		_ = rows.Close()
	}()

	var cols []Collection
	for rows.Next() {
		var projectID sql.NullString
		var data string
		if err := rows.Scan(&projectID, &data); err != nil {
			return nil, err
		}
		var col Collection
		if err := json.Unmarshal([]byte(data), &col); err != nil {
			return nil, err
		}
		if col.ProjectID == "" && projectID.Valid {
			col.ProjectID = projectID.String
		}
		cols = append(cols, col)
	}
	return cols, rows.Err()
}

func (s *Store) GetCollection(id string) (*Collection, error) {
	var projectID sql.NullString
	var data string
	err := s.db.QueryRow(`SELECT project_id, data FROM collections WHERE id = ?`, id).Scan(&projectID, &data)
	if err != nil {
		return nil, err
	}
	var col Collection
	if err := json.Unmarshal([]byte(data), &col); err != nil {
		return nil, err
	}
	if col.ProjectID == "" && projectID.Valid {
		col.ProjectID = projectID.String
	}
	return &col, nil
}

func (s *Store) DeleteCollection(id string) error {
	_, err := s.db.Exec(`DELETE FROM collections WHERE id = ?`, id)
	return err
}

// History operations
func (s *Store) AddHistory(h *History) error {
	snapshot, err := json.Marshal(h.RequestSnapshot)
	if err != nil {
		return err
	}

	_, err = s.db.Exec(
		`INSERT INTO history (id, project_id, timestamp, service, method, address, status, duration, request_snapshot)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		h.ID, h.ProjectID, h.Timestamp, h.Service, h.Method, h.Address, h.Status, h.Duration, string(snapshot),
	)
	return err
}

func (s *Store) GetHistories(limit, offset int) ([]History, error) {
	if limit <= 0 {
		limit = 100
	}
	if offset < 0 {
		offset = 0
	}

	query := `SELECT id, project_id, timestamp, service, method, address, status, duration, request_snapshot
	          FROM history ORDER BY timestamp DESC LIMIT ? OFFSET ?`

	rows, err := s.db.Query(query, limit, offset)
	if err != nil {
		return nil, err
	}
	defer func() {
		_ = rows.Close()
	}()

	var histories []History
	for rows.Next() {
		var h History
		var snapshot string
		err := rows.Scan(&h.ID, &h.ProjectID, &h.Timestamp, &h.Service, &h.Method, &h.Address, &h.Status, &h.Duration, &snapshot)
		if err != nil {
			return nil, err
		}
		if err := json.Unmarshal([]byte(snapshot), &h.RequestSnapshot); err != nil {
			return nil, err
		}
		histories = append(histories, h)
	}
	return histories, rows.Err()
}

func (s *Store) BatchInsertHistory(entries []HistoryEntry) error {
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	defer func() {
		_ = tx.Rollback()
	}()

	stmt, err := tx.Prepare(
		`INSERT INTO history (id, project_id, timestamp, service, method, address, status, duration, request_snapshot)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
	)
	if err != nil {
		return err
	}
	defer func() {
		_ = stmt.Close()
	}()

	for _, entry := range entries {
		snapshot, err := json.Marshal(entry.RequestSnapshot)
		if err != nil {
			return err
		}
		_, err = stmt.Exec(entry.ID, entry.ProjectID, entry.Timestamp, entry.Service, entry.Method, entry.Address, entry.Status, entry.Duration, string(snapshot))
		if err != nil {
			return err
		}
	}

	return tx.Commit()
}

func (s *Store) SearchHistory(query string, filters Filters) ([]History, error) {
	var conditions []string
	var args []interface{}

	if query != "" {
		searchPattern := "%" + strings.ToLower(query) + "%"
		conditions = append(conditions, "(LOWER(service) LIKE ? OR LOWER(method) LIKE ?)")
		args = append(args, searchPattern, searchPattern)
	}

	if filters.Service != "" {
		conditions = append(conditions, "service = ?")
		args = append(args, filters.Service)
	}

	if filters.Method != "" {
		conditions = append(conditions, "method = ?")
		args = append(args, filters.Method)
	}

	if filters.Status != "" {
		conditions = append(conditions, "status = ?")
		args = append(args, filters.Status)
	}

	if filters.StartTime > 0 {
		conditions = append(conditions, "timestamp >= ?")
		args = append(args, filters.StartTime)
	}

	if filters.EndTime > 0 {
		conditions = append(conditions, "timestamp <= ?")
		args = append(args, filters.EndTime)
	}

	sqlQuery := `SELECT id, project_id, timestamp, service, method, address, status, duration, request_snapshot FROM history`
	if len(conditions) > 0 {
		sqlQuery += " WHERE " + strings.Join(conditions, " AND ")
	}
	sqlQuery += " ORDER BY timestamp DESC"

	if filters.Limit > 0 {
		sqlQuery += fmt.Sprintf(" LIMIT %d", filters.Limit)
		if filters.Offset > 0 {
			sqlQuery += fmt.Sprintf(" OFFSET %d", filters.Offset)
		}
	}

	rows, err := s.db.Query(sqlQuery, args...)
	if err != nil {
		return nil, err
	}
	defer func() {
		_ = rows.Close()
	}()

	var histories []History
	for rows.Next() {
		var h History
		var snapshot string
		err := rows.Scan(&h.ID, &h.ProjectID, &h.Timestamp, &h.Service, &h.Method, &h.Address, &h.Status, &h.Duration, &snapshot)
		if err != nil {
			return nil, err
		}
		if err := json.Unmarshal([]byte(snapshot), &h.RequestSnapshot); err != nil {
			return nil, err
		}
		histories = append(histories, h)
	}
	return histories, rows.Err()
}

func (s *Store) DeleteHistory(id string) error {
	_, err := s.db.Exec(`DELETE FROM history WHERE id = ?`, id)
	return err
}

func (s *Store) ClearHistory() error {
	_, err := s.db.Exec(`DELETE FROM history`)
	return err
}

// Export/Import operations
func (s *Store) ExportCollections() ([]byte, error) {
	cols, err := s.GetCollections()
	if err != nil {
		return nil, err
	}
	return json.MarshalIndent(cols, "", "  ")
}

func (s *Store) ImportCollections(data []byte) error {
	var cols []Collection
	if err := json.Unmarshal(data, &cols); err != nil {
		return fmt.Errorf("invalid collection data: %w", err)
	}

	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	defer func() {
		_ = tx.Rollback()
	}()

	for _, col := range cols {
		colData, err := json.Marshal(col)
		if err != nil {
			return err
		}
		_, err = tx.Exec(
			`INSERT INTO collections (id, project_id, name, data, created_at, updated_at)
			 VALUES (?, ?, ?, ?, ?, ?)
			 ON CONFLICT(id) DO UPDATE SET
			 project_id=excluded.project_id,
			 name=excluded.name,
			 data=excluded.data,
			 updated_at=excluded.updated_at`,
			col.ID, col.ProjectID, col.Name, string(colData), time.Now(), time.Now(),
		)
		if err != nil {
			return err
		}
	}

	return tx.Commit()
}

func (s *Store) ExportEnvironments() ([]byte, error) {
	envs, err := s.GetEnvironments()
	if err != nil {
		return nil, err
	}
	return json.MarshalIndent(envs, "", "  ")
}

func (s *Store) ImportEnvironments(data []byte) error {
	var envs []Environment
	if err := json.Unmarshal(data, &envs); err != nil {
		return fmt.Errorf("invalid environment data: %w", err)
	}

	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	defer func() {
		_ = tx.Rollback()
	}()

	for _, env := range envs {
		variables, err := json.Marshal(env.Variables)
		if err != nil {
			return err
		}
		headers, err := json.Marshal(env.Headers)
		if err != nil {
			return err
		}
		tlsConfig, err := json.Marshal(env.TLSConfig)
		if err != nil {
			return err
		}

		_, err = tx.Exec(
			`INSERT INTO environments (id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
			 ON CONFLICT(id) DO UPDATE SET
			 project_id=excluded.project_id,
			 name=excluded.name,
			 base_url=excluded.base_url,
			 variables=excluded.variables,
			 headers=excluded.headers,
			 tls_config=excluded.tls_config,
			 is_default=excluded.is_default,
			 updated_at=excluded.updated_at`,
			env.ID, env.ProjectID, env.Name, env.BaseURL, string(variables), string(headers), string(tlsConfig),
			env.IsDefault, env.CreatedAt, time.Now(),
		)
		if err != nil {
			return err
		}
	}

	return tx.Commit()
}
