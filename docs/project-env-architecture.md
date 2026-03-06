# 项目-环境架构设计方案

## 1. 数据模型设计

### 1.1 Project 模型
```go
// sidecar/internal/storage/models.go
type Project struct {
    ID                   string    `json:"id"`
    Name                 string    `json:"name"`
    Description          string    `json:"description"`
    DefaultEnvironmentID string    `json:"default_environment_id,omitempty"`
    ProtoFiles           []string  `json:"proto_files,omitempty"`
    CreatedAt            time.Time `json:"created_at"`
    UpdatedAt            time.Time `json:"updated_at"`
}
```

### 1.2 修改后的 Environment 模型
```go
type Environment struct {
    ID        string            `json:"id"`
    ProjectID string            `json:"project_id"`  // 新增：归属项目
    Name      string            `json:"name"`
    BaseURL   string            `json:"base_url"`
    Variables map[string]string `json:"variables"`
    Headers   map[string]string `json:"headers"`
    TLSConfig *TLSConfig        `json:"tls_config,omitempty"`
    IsDefault bool              `json:"is_default"`  // 新增
    CreatedAt time.Time         `json:"created_at"`
    UpdatedAt time.Time         `json:"updated_at"`
}
```

### 1.3 修改后的 Collection 模型
```go
type Collection struct {
    ID        string        `json:"id"`
    ProjectID string        `json:"project_id"`  // 新增
    Name      string        `json:"name"`
    Folders   []Folder      `json:"folders"`
    Items     []RequestItem `json:"items"`
    CreatedAt time.Time     `json:"created_at"`
    UpdatedAt time.Time     `json:"updated_at"`
}
```

### 1.4 修改后的 RequestItem 模型
```go
type RequestItem struct {
    ID         string            `json:"id"`
    Name       string            `json:"name"`
    Type       string            `json:"type"`
    Service    string            `json:"service"`
    Method     string            `json:"method"`
    Body       string            `json:"body"`
    Metadata   map[string]string `json:"metadata"`
    EnvRefType string            `json:"env_ref_type"` // "inherit" | "specific" | "none"
    EnvironmentID string         `json:"environment_id,omitempty"`
}
```

## 2. 数据库 Schema 变更

```sql
-- 创建 projects 表
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    default_environment_id TEXT,
    proto_files TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 创建 project_environments 关联表（多对多）
CREATE TABLE IF NOT EXISTS project_environments (
    project_id TEXT NOT NULL,
    environment_id TEXT NOT NULL,
    is_default BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (project_id, environment_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (environment_id) REFERENCES environments(id) ON DELETE CASCADE
);

-- 修改 environments 表
ALTER TABLE environments ADD COLUMN project_id TEXT REFERENCES projects(id);
CREATE INDEX idx_environments_project ON environments(project_id);

-- 修改 collections 表
ALTER TABLE collections ADD COLUMN project_id TEXT REFERENCES projects(id);
CREATE INDEX idx_collections_project ON collections(project_id);

-- 修改 history 表
ALTER TABLE history ADD COLUMN project_id TEXT;
CREATE INDEX idx_history_project ON history(project_id);
```

## 3. Go Sidecar API 变更

### 3.1 新增 Project 路由
```go
// api/routes.go 新增路由
mux.HandleFunc("/projects", r.handleProjects)
mux.HandleFunc("/projects/", r.handleProjectByID)
```

### 3.2 Project Handler 实现
```go
// api/project_handlers.go
func (r *Router) handleProjects(w http.ResponseWriter, req *http.Request) {
    switch req.Method {
    case http.MethodGet:
        // GET /projects - 列出所有项目
        projects, err := r.store.GetProjects()
        // ...
    case http.MethodPost:
        // POST /projects - 创建项目
        var project storage.Project
        // ...
    }
}

func (r *Router) handleProjectByID(w http.ResponseWriter, req *http.Request) {
    // GET /projects/:id - 获取项目详情
    // PUT /projects/:id - 更新项目
    // DELETE /projects/:id - 删除项目
    // POST /projects/:id/clone - 克隆项目
}
```

### 3.3 Storage 层新增方法
```go
// internal/storage/store.go

// Project operations
func (s *Store) SaveProject(project *Project) error
func (s *Store) GetProjects() ([]Project, error)
func (s *Store) GetProject(id string) (*Project, error)
func (s *Store) DeleteProject(id string) error
func (s *Store) CloneProject(id string, newName string) (*Project, error)

// Environment operations (modified)
func (s *Store) GetEnvironmentsByProject(projectID string) ([]Environment, error)
func (s *Store) SetDefaultEnvironment(projectID, envID string) error

// Collection operations (modified)
func (s *Store) GetCollectionsByProject(projectID string) ([]Collection, error)
```

## 4. Rust Tauri 命令变更

### 4.1 新增 Project 命令
```rust
// src-tauri/src/commands.rs

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    id: String,
    name: String,
    description: String,
    default_environment_id: Option<String>,
}

#[tauri::command]
pub async fn get_projects(state: State<'_, AppState>) -> Result<Vec<Project>>

#[tauri::command]
pub async fn create_project(
    state: State<'_, AppState>,
    project: Project,
) -> Result<Project>

#[tauri::command]
pub async fn update_project(
    state: State<'_, AppState>,
    project: Project,
) -> Result<()>

#[tauri::command]
pub async fn delete_project(
    state: State<'_, AppState>,
    id: String,
) -> Result<()>

#[tauri::command]
pub async fn clone_project(
    state: State<'_, AppState>,
    id: String,
    new_name: String,
) -> Result<Project>
```

### 4.2 修改现有命令
```rust
// 修改环境相关命令，添加 project_id 参数
#[tauri::command]
pub async fn get_environments(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<Environment>>

#[tauri::command]
pub async fn save_environment(
    state: State<'_, AppState>,
    project_id: String,
    env: Environment,
) -> Result<()>

// 修改集合相关命令
#[tauri::command]
pub async fn get_collections(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<Collection>>
```

## 5. 前端 React 组件变更

### 5.1 新增组件
```typescript
// src/components/project/
├── ProjectSelector.tsx      # 项目选择器（下拉/列表）
├── ProjectManager.tsx       # 项目管理页面
├── ProjectCreateDialog.tsx  # 创建项目对话框
├── ProjectSettings.tsx      # 项目设置

// src/components/environment/
├── EnvironmentSelector.tsx  # 环境选择器（带继承选项）
├── EnvironmentList.tsx      # 项目内环境列表
├── EnvironmentCloneDialog.tsx # 克隆环境对话框
```

### 5.2 修改现有组件
```typescript
// src/components/RequestPanel.tsx
// 添加环境引用类型选择
interface RequestPanelProps {
  request: RequestItem;
  projectId: string;
  onEnvRefChange: (type: 'inherit' | 'specific' | 'none', envId?: string) => void;
}

// src/components/Sidebar.tsx
// 改为项目-集合树形结构
interface SidebarProps {
  projects: Project[];
  activeProjectId: string;
  onProjectSelect: (id: string) => void;
}
```

### 5.3 状态管理变更
```typescript
// src/store/projectStore.ts
interface ProjectState {
  projects: Project[];
  currentProject: Project | null;
  environments: Environment[];
  collections: Collection[];
  activeEnvironmentId: string | null;
}

interface ProjectActions {
  loadProjects: () => Promise<void>;
  selectProject: (id: string) => Promise<void>;
  createProject: (data: CreateProjectData) => Promise<Project>;
  cloneProject: (id: string, newName: string) => Promise<Project>;
  setDefaultEnvironment: (projectId: string, envId: string) => Promise<void>;
}
```

## 6. 测试策略

### 6.1 后端测试
```go
// sidecar/internal/storage/store_test.go
func TestProjectOperations(t *testing.T)
func TestEnvironmentWithProject(t *testing.T)
func TestCloneProject(t *testing.T)

// sidecar/api/routes_test.go
func TestProjectHandlers(t *testing.T)
func TestEnvironmentInProjectContext(t *testing.T)
```

### 6.2 前端测试
```typescript
// src/components/project/__tests__/
├── ProjectSelector.test.tsx
├── ProjectManager.test.tsx

// src/store/__tests__/
├── projectStore.test.ts
```

### 6.3 E2E 测试
```typescript
// tests/e2e/project.test.ts
test('create project with environments', async () => {
  // 创建项目
  // 添加环境
  // 验证环境继承
});

test('switch environment in request', async () => {
  // 创建请求
  // 切换环境
  // 验证变量解析
});
```

## 7. 实施顺序

### Phase 1: 后端基础
1. 修改数据库 schema (migrations)
2. 新增 Project 模型和存储方法
3. 修改 Environment/Collection 存储方法
4. 新增 Project API 路由

### Phase 2: Rust 层
1. 新增 Project 命令
2. 修改现有命令添加 project_id 参数
3. 更新命令注册

### Phase 3: 前端基础
1. 新增 projectStore
2. 新增 ProjectSelector 组件
3. 修改 Sidebar 组件

### Phase 4: 功能完善
1. 环境继承逻辑
2. 项目克隆功能
3. 导入/导出功能

### Phase 5: 测试
1. 单元测试
2. 集成测试
3. E2E 测试
