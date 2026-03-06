package storage

import (
	"path/filepath"
	"testing"
	"time"
)

// newTestStore 创建一个隔离的 SQLite 存储实例，
// 每个测试都会使用独立数据库文件，避免测试间数据污染。
func newTestStore(t *testing.T) *Store {
	t.Helper()

	dbPath := filepath.Join(t.TempDir(), "grpcui-test.db")
	store, err := NewSQLiteStore(dbPath)
	if err != nil {
		t.Fatalf("failed to create test store: %v", err)
	}

	t.Cleanup(func() {
		_ = store.Close()
	})

	return store
}

// mustSaveProject 是测试辅助函数，
// 用于快速写入项目并在失败时立刻终止测试。
func mustSaveProject(t *testing.T, store *Store, project *Project) {
	t.Helper()
	if err := store.SaveProject(project); err != nil {
		t.Fatalf("failed to save project: %v", err)
	}
}

// mustSaveEnvironment 是测试辅助函数，
// 用于快速写入环境并在失败时立刻终止测试。
func mustSaveEnvironment(t *testing.T, store *Store, env *Environment) {
	t.Helper()
	if err := store.SaveEnvironment(env); err != nil {
		t.Fatalf("failed to save environment: %v", err)
	}
}

// mustSaveCollection 是测试辅助函数，
// 用于快速写入集合并在失败时立刻终止测试。
func mustSaveCollection(t *testing.T, store *Store, col *Collection) {
	t.Helper()
	if err := store.SaveCollection(col); err != nil {
		t.Fatalf("failed to save collection: %v", err)
	}
}

// TestProjectScopedOperations 验证项目域能力：
// 1) Project CRUD 可用；
// 2) Environment / Collection 支持按项目隔离查询；
// 3) 设置默认环境后能够正确回写项目与环境状态。
func TestProjectScopedOperations(t *testing.T) {
	store := newTestStore(t)
	now := time.Now().UTC()

	projectA := &Project{
		ID:          "project-a",
		Name:        "Project A",
		Description: "first project",
		ProtoFiles:  []string{"a.proto"},
		CreatedAt:   now,
		UpdatedAt:   now,
	}
	projectB := &Project{
		ID:          "project-b",
		Name:        "Project B",
		Description: "second project",
		ProtoFiles:  []string{"b.proto"},
		CreatedAt:   now,
		UpdatedAt:   now,
	}
	mustSaveProject(t, store, projectA)
	mustSaveProject(t, store, projectB)

	envA1 := &Environment{
		ID:        "env-a-1",
		ProjectID: projectA.ID,
		Name:      "A-Dev",
		BaseURL:   "localhost:50051",
		Variables: map[string]string{"TOKEN": "a"},
		Headers:   map[string]string{"x-project": "a"},
		CreatedAt: now,
		UpdatedAt: now,
	}
	envA2 := &Environment{
		ID:        "env-a-2",
		ProjectID: projectA.ID,
		Name:      "A-Prod",
		BaseURL:   "localhost:50052",
		Variables: map[string]string{"TOKEN": "a2"},
		Headers:   map[string]string{"x-project": "a2"},
		CreatedAt: now,
		UpdatedAt: now,
	}
	envB1 := &Environment{
		ID:        "env-b-1",
		ProjectID: projectB.ID,
		Name:      "B-Dev",
		BaseURL:   "localhost:50053",
		Variables: map[string]string{"TOKEN": "b"},
		Headers:   map[string]string{"x-project": "b"},
		CreatedAt: now,
		UpdatedAt: now,
	}
	mustSaveEnvironment(t, store, envA1)
	mustSaveEnvironment(t, store, envA2)
	mustSaveEnvironment(t, store, envB1)

	colA := &Collection{
		ID:        "col-a",
		ProjectID: projectA.ID,
		Name:      "Collection A",
		Items: []RequestItem{{
			ID:      "req-a",
			Name:    "Request A",
			Type:    "unary",
			Service: "pkg.Service",
			Method:  "CallA",
			Body:    `{}`,
		}},
		CreatedAt: now,
		UpdatedAt: now,
	}
	colB := &Collection{
		ID:        "col-b",
		ProjectID: projectB.ID,
		Name:      "Collection B",
		Items: []RequestItem{{
			ID:      "req-b",
			Name:    "Request B",
			Type:    "unary",
			Service: "pkg.Service",
			Method:  "CallB",
			Body:    `{}`,
		}},
		CreatedAt: now,
		UpdatedAt: now,
	}
	mustSaveCollection(t, store, colA)
	mustSaveCollection(t, store, colB)

	envsOfA, err := store.GetEnvironmentsByProject(projectA.ID)
	if err != nil {
		t.Fatalf("failed to get environments by project: %v", err)
	}
	if len(envsOfA) != 2 {
		t.Fatalf("expected 2 environments in project A, got %d", len(envsOfA))
	}

	colsOfA, err := store.GetCollectionsByProject(projectA.ID)
	if err != nil {
		t.Fatalf("failed to get collections by project: %v", err)
	}
	if len(colsOfA) != 1 {
		t.Fatalf("expected 1 collection in project A, got %d", len(colsOfA))
	}
	if colsOfA[0].ID != colA.ID {
		t.Fatalf("expected collection %s, got %s", colA.ID, colsOfA[0].ID)
	}

	if err := store.SetDefaultEnvironment(projectA.ID, envA2.ID); err != nil {
		t.Fatalf("failed to set default environment: %v", err)
	}

	updatedProject, err := store.GetProject(projectA.ID)
	if err != nil {
		t.Fatalf("failed to load updated project: %v", err)
	}
	if updatedProject.DefaultEnvironmentID != envA2.ID {
		t.Fatalf("expected default environment %s, got %s", envA2.ID, updatedProject.DefaultEnvironmentID)
	}

	envsOfAAfterDefault, err := store.GetEnvironmentsByProject(projectA.ID)
	if err != nil {
		t.Fatalf("failed to reload project environments: %v", err)
	}
	defaultCount := 0
	for _, env := range envsOfAAfterDefault {
		if env.IsDefault {
			defaultCount++
		}
	}
	if defaultCount != 1 {
		t.Fatalf("expected exactly one default environment, got %d", defaultCount)
	}
}

// TestCloneProject 验证项目克隆行为：
// 1) 克隆出的项目具备新 ID 与新名称；
// 2) 环境与集合会深拷贝到新项目；
// 3) 默认环境映射到克隆后的环境，而不是旧环境 ID。
func TestCloneProject(t *testing.T) {
	store := newTestStore(t)
	now := time.Now().UTC()

	source := &Project{
		ID:          "project-source",
		Name:        "Source",
		Description: "source project",
		ProtoFiles:  []string{"source.proto"},
		CreatedAt:   now,
		UpdatedAt:   now,
	}
	mustSaveProject(t, store, source)

	env := &Environment{
		ID:        "env-source",
		ProjectID: source.ID,
		Name:      "Source Env",
		BaseURL:   "localhost:50060",
		Variables: map[string]string{"K": "V"},
		Headers:   map[string]string{"x-env": "source"},
		CreatedAt: now,
		UpdatedAt: now,
	}
	mustSaveEnvironment(t, store, env)
	if err := store.SetDefaultEnvironment(source.ID, env.ID); err != nil {
		t.Fatalf("failed to set source default environment: %v", err)
	}

	col := &Collection{
		ID:        "col-source",
		ProjectID: source.ID,
		Name:      "Source Collection",
		Items: []RequestItem{{
			ID:            "req-source",
			Name:          "Request Source",
			Type:          "unary",
			Service:       "pkg.Service",
			Method:        "Say",
			Body:          `{"name":"source"}`,
			Metadata:      map[string]string{"x": "1"},
			EnvRefType:    "inherit",
			EnvironmentID: env.ID,
		}},
		CreatedAt: now,
		UpdatedAt: now,
	}
	mustSaveCollection(t, store, col)

	cloned, err := store.CloneProject(source.ID, "Source Copy")
	if err != nil {
		t.Fatalf("failed to clone project: %v", err)
	}

	if cloned.ID == source.ID {
		t.Fatalf("expected cloned project id to differ from source")
	}
	if cloned.Name != "Source Copy" {
		t.Fatalf("unexpected cloned project name: %s", cloned.Name)
	}

	clonedEnvs, err := store.GetEnvironmentsByProject(cloned.ID)
	if err != nil {
		t.Fatalf("failed to get cloned environments: %v", err)
	}
	if len(clonedEnvs) != 1 {
		t.Fatalf("expected 1 cloned environment, got %d", len(clonedEnvs))
	}
	if clonedEnvs[0].ID == env.ID {
		t.Fatalf("expected cloned environment id to differ from source")
	}

	clonedCols, err := store.GetCollectionsByProject(cloned.ID)
	if err != nil {
		t.Fatalf("failed to get cloned collections: %v", err)
	}
	if len(clonedCols) != 1 {
		t.Fatalf("expected 1 cloned collection, got %d", len(clonedCols))
	}
	if clonedCols[0].ID == col.ID {
		t.Fatalf("expected cloned collection id to differ from source")
	}
	if len(clonedCols[0].Items) != 1 {
		t.Fatalf("expected 1 cloned request item, got %d", len(clonedCols[0].Items))
	}

	if cloned.DefaultEnvironmentID == "" {
		t.Fatalf("expected cloned default environment id to be set")
	}
	if cloned.DefaultEnvironmentID == env.ID {
		t.Fatalf("expected cloned default environment id to reference cloned env")
	}
}
