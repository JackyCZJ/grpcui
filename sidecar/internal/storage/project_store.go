package storage

import (
	"crypto/rand"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"time"
)

// newStorageID 生成存储层统一使用的随机 ID，
// 使用 16 字节随机数编码为 32 位十六进制字符串，
// 在数据库层避免因时间戳并发冲突导致的主键碰撞。
func newStorageID() string {
	buf := make([]byte, 16)
	if _, err := rand.Read(buf); err != nil {
		return fmt.Sprintf("fallback-%d", time.Now().UnixNano())
	}
	return hex.EncodeToString(buf)
}

// SaveProject 保存或更新项目基础信息，
// 同时序列化 proto_files 字段并保证 updated_at 总是反映最新修改时间。
func (s *Store) SaveProject(project *Project) error {
	if project == nil {
		return errors.New("project is nil")
	}
	if project.ID == "" {
		project.ID = newStorageID()
	}
	if project.Name == "" {
		return errors.New("project name is required")
	}

	protoFiles, err := json.Marshal(project.ProtoFiles)
	if err != nil {
		return fmt.Errorf("failed to marshal proto files: %w", err)
	}

	now := time.Now()
	if project.CreatedAt.IsZero() {
		project.CreatedAt = now
	}
	project.UpdatedAt = now

	_, err = s.db.Exec(
		`INSERT INTO projects (id, name, description, default_environment_id, proto_files, created_at, updated_at)
		 VALUES (?, ?, ?, ?, ?, ?, ?)
		 ON CONFLICT(id) DO UPDATE SET
		 name=excluded.name,
		 description=excluded.description,
		 default_environment_id=excluded.default_environment_id,
		 proto_files=excluded.proto_files,
		 updated_at=excluded.updated_at`,
		project.ID,
		project.Name,
		project.Description,
		project.DefaultEnvironmentID,
		string(protoFiles),
		project.CreatedAt,
		project.UpdatedAt,
	)
	return err
}

// GetProjects 查询全部项目并按名称排序返回，
// 同时反序列化 proto_files，保证前端可直接消费结构化数组。
func (s *Store) GetProjects() ([]Project, error) {
	rows, err := s.db.Query(`
		SELECT id, name, description, default_environment_id, proto_files, created_at, updated_at
		FROM projects
		ORDER BY name
	`)
	if err != nil {
		return nil, err
	}
	defer func() {
		_ = rows.Close()
	}()

	projects := make([]Project, 0)
	for rows.Next() {
		var p Project
		var defaultEnvironmentID sql.NullString
		var protoFiles string
		if err := rows.Scan(
			&p.ID,
			&p.Name,
			&p.Description,
			&defaultEnvironmentID,
			&protoFiles,
			&p.CreatedAt,
			&p.UpdatedAt,
		); err != nil {
			return nil, err
		}
		if defaultEnvironmentID.Valid {
			p.DefaultEnvironmentID = defaultEnvironmentID.String
		}
		if protoFiles != "" {
			if err := json.Unmarshal([]byte(protoFiles), &p.ProtoFiles); err != nil {
				p.ProtoFiles = []string{}
			}
		}
		projects = append(projects, p)
	}

	if err := rows.Err(); err != nil {
		return nil, err
	}

	return projects, nil
}

// GetProject 根据 ID 获取单个项目详情，
// 未命中时返回 sql.ErrNoRows，便于上层路由映射 404 语义。
func (s *Store) GetProject(id string) (*Project, error) {
	var p Project
	var defaultEnvironmentID sql.NullString
	var protoFiles string
	if err := s.db.QueryRow(
		`SELECT id, name, description, default_environment_id, proto_files, created_at, updated_at FROM projects WHERE id = ?`,
		id,
	).Scan(
		&p.ID,
		&p.Name,
		&p.Description,
		&defaultEnvironmentID,
		&protoFiles,
		&p.CreatedAt,
		&p.UpdatedAt,
	); err != nil {
		return nil, err
	}

	if defaultEnvironmentID.Valid {
		p.DefaultEnvironmentID = defaultEnvironmentID.String
	}
	if protoFiles != "" {
		if err := json.Unmarshal([]byte(protoFiles), &p.ProtoFiles); err != nil {
			p.ProtoFiles = []string{}
		}
	}

	return &p, nil
}

// DeleteProject 删除项目并清理关联数据，
// 由于部分历史数据库可能未启用外键级联，这里显式执行子表清理，
// 以保证删除操作在任何环境中结果一致。
func (s *Store) DeleteProject(id string) error {
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	defer func() {
		_ = tx.Rollback()
	}()

	queries := []struct {
		sql  string
		args []interface{}
	}{
		{sql: `DELETE FROM project_environments WHERE project_id = ?`, args: []interface{}{id}},
		{sql: `DELETE FROM history WHERE project_id = ?`, args: []interface{}{id}},
		{sql: `DELETE FROM collections WHERE project_id = ?`, args: []interface{}{id}},
		{sql: `DELETE FROM environments WHERE project_id = ?`, args: []interface{}{id}},
		{sql: `DELETE FROM projects WHERE id = ?`, args: []interface{}{id}},
	}

	for _, query := range queries {
		if _, err := tx.Exec(query.sql, query.args...); err != nil {
			return err
		}
	}

	return tx.Commit()
}

// GetEnvironmentsByProject 查询项目下的环境列表，
// 返回结果按默认环境优先再按名称排序，方便 UI 直接展示。
func (s *Store) GetEnvironmentsByProject(projectID string) ([]Environment, error) {
	rows, err := s.db.Query(`
		SELECT id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at
		FROM environments
		WHERE project_id = ?
		ORDER BY is_default DESC, name
	`, projectID)
	if err != nil {
		return nil, err
	}
	defer func() {
		_ = rows.Close()
	}()

	envs := make([]Environment, 0)
	for rows.Next() {
		var env Environment
		var variables sql.NullString
		var headers sql.NullString
		var tlsConfig sql.NullString
		if err := rows.Scan(
			&env.ID,
			&env.ProjectID,
			&env.Name,
			&env.BaseURL,
			&variables,
			&headers,
			&tlsConfig,
			&env.IsDefault,
			&env.CreatedAt,
			&env.UpdatedAt,
		); err != nil {
			return nil, err
		}
		if err := json.Unmarshal([]byte(variables.String), &env.Variables); err != nil {
			env.Variables = map[string]string{}
		}
		if err := json.Unmarshal([]byte(headers.String), &env.Headers); err != nil {
			env.Headers = map[string]string{}
		}
		if err := json.Unmarshal([]byte(tlsConfig.String), &env.TLSConfig); err != nil {
			env.TLSConfig = nil
		}
		envs = append(envs, env)
	}

	if err := rows.Err(); err != nil {
		return nil, err
	}

	return envs, nil
}

// SetDefaultEnvironment 设置项目默认环境，
// 该操作会在同一事务内完成三件事：
// 1) 清空项目内其他环境的默认标记；
// 2) 标记目标环境为默认；
// 3) 回写 projects.default_environment_id，确保查询一致性。
func (s *Store) SetDefaultEnvironment(projectID, envID string) error {
	tx, err := s.db.Begin()
	if err != nil {
		return err
	}
	defer func() {
		_ = tx.Rollback()
	}()

	var count int
	if err := tx.QueryRow(
		`SELECT COUNT(*) FROM environments WHERE id = ? AND project_id = ?`,
		envID,
		projectID,
	).Scan(&count); err != nil {
		return err
	}
	if count == 0 {
		return sql.ErrNoRows
	}

	if _, err := tx.Exec(`UPDATE environments SET is_default = FALSE WHERE project_id = ?`, projectID); err != nil {
		return err
	}
	if _, err := tx.Exec(
		`UPDATE environments SET is_default = TRUE, updated_at = ? WHERE id = ? AND project_id = ?`,
		time.Now(),
		envID,
		projectID,
	); err != nil {
		return err
	}

	if _, err := tx.Exec(`UPDATE project_environments SET is_default = FALSE WHERE project_id = ?`, projectID); err != nil {
		return err
	}
	if _, err := tx.Exec(
		`INSERT INTO project_environments (project_id, environment_id, is_default)
		 VALUES (?, ?, TRUE)
		 ON CONFLICT(project_id, environment_id) DO UPDATE SET is_default=TRUE`,
		projectID,
		envID,
	); err != nil {
		return err
	}

	if _, err := tx.Exec(
		`UPDATE projects SET default_environment_id = ?, updated_at = ? WHERE id = ?`,
		envID,
		time.Now(),
		projectID,
	); err != nil {
		return err
	}

	return tx.Commit()
}

// GetCollectionsByProject 查询项目下的集合。
// 若历史数据的 JSON 里缺失 project_id，会自动回填为当前查询项目，
// 以保证前端读取结构稳定。
func (s *Store) GetCollectionsByProject(projectID string) ([]Collection, error) {
	rows, err := s.db.Query(`SELECT project_id, data FROM collections WHERE project_id = ? ORDER BY name`, projectID)
	if err != nil {
		return nil, err
	}
	defer func() {
		_ = rows.Close()
	}()

	collections := make([]Collection, 0)
	for rows.Next() {
		var dbProjectID sql.NullString
		var data string
		if err := rows.Scan(&dbProjectID, &data); err != nil {
			return nil, err
		}
		var collection Collection
		if err := json.Unmarshal([]byte(data), &collection); err != nil {
			return nil, err
		}
		if collection.ProjectID == "" {
			if dbProjectID.Valid {
				collection.ProjectID = dbProjectID.String
			} else {
				collection.ProjectID = projectID
			}
		}
		collections = append(collections, collection)
	}

	if err := rows.Err(); err != nil {
		return nil, err
	}

	return collections, nil
}

// CloneProject 克隆项目及其关联环境、集合，
// 并维护默认环境映射关系，确保克隆后的请求项仍然引用新环境 ID。
func (s *Store) CloneProject(id string, newName string) (*Project, error) {
	source, err := s.GetProject(id)
	if err != nil {
		return nil, err
	}

	tx, err := s.db.Begin()
	if err != nil {
		return nil, err
	}
	defer func() {
		_ = tx.Rollback()
	}()

	now := time.Now()
	newProjectID := newStorageID()
	protoFiles, err := json.Marshal(source.ProtoFiles)
	if err != nil {
		return nil, err
	}

	if _, err := tx.Exec(
		`INSERT INTO projects (id, name, description, default_environment_id, proto_files, created_at, updated_at)
		 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		newProjectID,
		newName,
		source.Description,
		"",
		string(protoFiles),
		now,
		now,
	); err != nil {
		return nil, err
	}

	envIDMap, clonedDefaultEnvID, err := cloneProjectEnvironments(tx, source.ID, newProjectID)
	if err != nil {
		return nil, err
	}
	if source.DefaultEnvironmentID != "" {
		if mapped, ok := envIDMap[source.DefaultEnvironmentID]; ok {
			clonedDefaultEnvID = mapped
		}
	}

	if err := cloneProjectCollections(tx, source.ID, newProjectID, envIDMap); err != nil {
		return nil, err
	}

	if clonedDefaultEnvID != "" {
		if _, err := tx.Exec(`UPDATE environments SET is_default = TRUE WHERE id = ? AND project_id = ?`, clonedDefaultEnvID, newProjectID); err != nil {
			return nil, err
		}
		if _, err := tx.Exec(`
			INSERT INTO project_environments (project_id, environment_id, is_default)
			VALUES (?, ?, TRUE)
			ON CONFLICT(project_id, environment_id) DO UPDATE SET is_default=TRUE
		`, newProjectID, clonedDefaultEnvID); err != nil {
			return nil, err
		}
		if _, err := tx.Exec(`UPDATE projects SET default_environment_id = ? WHERE id = ?`, clonedDefaultEnvID, newProjectID); err != nil {
			return nil, err
		}
	}

	if err := tx.Commit(); err != nil {
		return nil, err
	}

	return s.GetProject(newProjectID)
}

// cloneProjectEnvironments 克隆项目下全部环境，
// 返回旧环境 ID 到新环境 ID 的映射，供请求项环境引用重写使用。
func cloneProjectEnvironments(tx *sql.Tx, sourceProjectID, targetProjectID string) (map[string]string, string, error) {
	rows, err := tx.Query(`
		SELECT id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at
		FROM environments
		WHERE project_id = ?
	`, sourceProjectID)
	if err != nil {
		return nil, "", err
	}
	defer func() {
		_ = rows.Close()
	}()

	mapping := make(map[string]string)
	defaultID := ""

	for rows.Next() {
		var oldID string
		var name string
		var baseURL string
		var variables sql.NullString
		var headers sql.NullString
		var tlsConfig sql.NullString
		var isDefault bool
		var createdAt time.Time
		var updatedAt time.Time
		if err := rows.Scan(&oldID, &name, &baseURL, &variables, &headers, &tlsConfig, &isDefault, &createdAt, &updatedAt); err != nil {
			return nil, "", err
		}

		newID := newStorageID()
		mapping[oldID] = newID
		if isDefault {
			defaultID = newID
		}

		if _, err := tx.Exec(
			`INSERT INTO environments (id, project_id, name, base_url, variables, headers, tls_config, is_default, created_at, updated_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, FALSE, ?, ?)`,
			newID,
			targetProjectID,
			name,
			baseURL,
			variables.String,
			headers.String,
			tlsConfig.String,
			time.Now(),
			time.Now(),
		); err != nil {
			return nil, "", err
		}

		if _, err := tx.Exec(
			`INSERT INTO project_environments (project_id, environment_id, is_default)
			 VALUES (?, ?, FALSE)
			 ON CONFLICT(project_id, environment_id) DO UPDATE SET is_default=FALSE`,
			targetProjectID,
			newID,
		); err != nil {
			return nil, "", err
		}
	}

	if err := rows.Err(); err != nil {
		return nil, "", err
	}

	return mapping, defaultID, nil
}

// cloneProjectCollections 克隆项目下的集合，并同步重写请求项中的环境引用。
func cloneProjectCollections(tx *sql.Tx, sourceProjectID, targetProjectID string, envIDMap map[string]string) error {
	rows, err := tx.Query(`SELECT data FROM collections WHERE project_id = ?`, sourceProjectID)
	if err != nil {
		return err
	}
	defer func() {
		_ = rows.Close()
	}()

	for rows.Next() {
		var data string
		if err := rows.Scan(&data); err != nil {
			return err
		}

		var collection Collection
		if err := json.Unmarshal([]byte(data), &collection); err != nil {
			return err
		}

		now := time.Now()
		collection.ID = newStorageID()
		collection.ProjectID = targetProjectID
		collection.CreatedAt = now
		collection.UpdatedAt = now
		collection.Items = cloneRequestItems(collection.Items, envIDMap)

		for i := range collection.Folders {
			collection.Folders[i].ID = newStorageID()
			collection.Folders[i].Items = cloneRequestItems(collection.Folders[i].Items, envIDMap)
		}

		collectionData, err := json.Marshal(collection)
		if err != nil {
			return err
		}

		if _, err := tx.Exec(
			`INSERT INTO collections (id, project_id, name, data, created_at, updated_at)
			 VALUES (?, ?, ?, ?, ?, ?)`,
			collection.ID,
			collection.ProjectID,
			collection.Name,
			string(collectionData),
			collection.CreatedAt,
			collection.UpdatedAt,
		); err != nil {
			return err
		}
	}

	return rows.Err()
}

// cloneRequestItems 深拷贝请求项并生成新 ID，
// 同时将 environment_id 从源项目映射到目标项目。
func cloneRequestItems(items []RequestItem, envIDMap map[string]string) []RequestItem {
	cloned := make([]RequestItem, 0, len(items))
	for _, item := range items {
		copied := item
		copied.ID = newStorageID()
		if copied.EnvironmentID != "" {
			if mapped, ok := envIDMap[copied.EnvironmentID]; ok {
				copied.EnvironmentID = mapped
			} else {
				copied.EnvironmentID = ""
			}
		}
		cloned = append(cloned, copied)
	}
	return cloned
}
