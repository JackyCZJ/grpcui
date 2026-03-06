# M0: 基线契约梳理文档

本文档记录当前 gRPC UI 代码库的完整行为契约，作为后续迁移工作的基准。

## 1. 行为契约 (Behavioral Contracts)

### 1.1 connect - gRPC 连接建立

**端点**: `POST /connect`

**输入参数**:
| 字段 | 类型 | 必需 | 验证规则 |
|------|------|------|----------|
| address | string | 是 | 非空，格式为 `host:port` |
| tls | object | 否 | TLS 配置对象 |
| insecure | bool | 否 | 默认 false，允许不安全连接 |
| proto_file | string | 否 | proto 文件路径 |
| use_reflection | bool | 否 | 默认 false，使用服务端反射 |

**TLS 配置结构**:
```json
{
  "mode": "string",        // TLS 模式
  "ca_cert": "string",     // CA 证书路径
  "client_cert": "string", // 客户端证书路径
  "client_key": "string",  // 客户端密钥路径
  "skip_verify": bool      // 跳过证书验证
}
```

**输出结构**:
```json
{
  "success": true,
  "address": "string",
  "connected": true,
  "has_proto": true,
  "proto_source": "file|reflection|"
}
```

**错误代码**:
- `400 Bad Request`: 请求体解析失败
- `405 Method Not Allowed`: 非 POST 请求
- `500 Internal Server Error`: 连接失败或 proto 文件加载失败

**边界行为**:
- 新连接会关闭已有连接
- proto_file 和 use_reflection 同时存在时优先使用 proto_file
- 反射失败不会导致连接失败，仅设置 has_proto=false
- 默认连接超时: 10秒

### 1.2 services - 列出可用服务/方法

**端点**: `GET /services`

**输入参数**: 无

**输出结构**:
```json
{
  "services": [
    {
      "name": "string",
      "full_name": "string",
      "methods": [
        {
          "name": "string",
          "full_name": "string",
          "input_type": "string",
          "output_type": "string",
          "type": "unary|server_stream|client_stream|bidi_stream",
          "client_streaming": true,
          "server_streaming": true
        }
      ]
    }
  ]
}
```

**错误代码**:
- `400 Bad Request`: 未加载 proto 定义
- `405 Method Not Allowed`: 非 GET 请求

### 1.3 invoke - 一元 gRPC 调用

**端点**: `POST /invoke`

**输入参数**:
| 字段 | 类型 | 必需 | 说明 |
|------|------|------|------|
| service | string | 是 | 服务名，支持 `Service/Method` 格式 |
| method | string | 是 | 方法名 |
| body | object/string | 否 | 请求体，支持 JSON 对象或字符串 |
| metadata | object | 否 | 键值对形式的 gRPC metadata |

**输出结构 (成功)**:
```json
{
  "data": {},
  "metadata": {},
  "duration": 123,
  "status": "OK"
}
```

**输出结构 (失败)**:
```json
{
  "error": "error message",
  "metadata": {},
  "duration": 123,
  "status": "ERROR"
}
```

**错误代码**:
- `400 Bad Request`: 未连接、未加载 proto、请求体解析失败
- `405 Method Not Allowed`: 非 POST 请求
- `500 Internal Server Error`: 调用失败

### 1.4 stream - 流式调用

#### 1.4.1 Server Stream (服务端流)

**端点**: `POST /invoke/server-stream`

**输入参数**: 同 invoke

**输出格式**: SSE (Server-Sent Events)

```
data: {"type": "message", "data": {...}}
data: {"type": "message", "data": {...}}
data: {"type": "end", "status": "OK"}
```

**错误响应**:
```
data: {"type": "error", "error": "message"}
```

#### 1.4.2 Client Stream (客户端流)

**端点**: `POST /invoke/client-stream`

**输入参数**:
```json
{
  "service": "string",
  "method": "string",
  "messages": [...],
  "metadata": {}
}
```

**输出结构**: 同 invoke 成功响应

**超时**: 30秒固定超时

#### 1.4.3 Bidirectional Stream (双向流)

**端点**: `POST /invoke/bidi-stream`

**输入参数**: 同 client-stream

**输出格式**: SSE，同 server-stream

### 1.5 projects - 项目 CRUD

#### 1.5.1 List Projects
**端点**: `GET /projects`

**输出**:
```json
[
  {
    "id": "string",
    "name": "string",
    "description": "string",
    "default_environment_id": "string",
    "proto_files": ["string"],
    "created_at": "RFC3339",
    "updated_at": "RFC3339"
  }
]
```

#### 1.5.2 Create Project
**端点**: `POST /projects`

**输入**:
```json
{
  "id": "string",          // 可选，自动生成
  "name": "string",        // 必需，非空
  "description": "string",
  "default_environment_id": "string",
  "proto_files": ["string"],
  "created_at": "RFC3339", // 可选
  "updated_at": "RFC3339"  // 自动设置
}
```

**输出**: 201 Created，返回创建的项目对象

**错误**:
- `400`: name 为空或请求体解析失败
- `500`: 数据库错误

#### 1.5.3 Get Project
**端点**: `GET /projects/{id}`

**错误**:
- `404`: 项目不存在

#### 1.5.4 Update Project
**端点**: `PUT /projects/{id}`

**行为**:
- 部分更新：未提供的字段保持原值
- 自动更新 updated_at

#### 1.5.5 Delete Project
**端点**: `DELETE /projects/{id}`

**输出**: 204 No Content

**级联删除**: 关联的 environments, collections, history

#### 1.5.6 Clone Project
**端点**: `POST /projects/{id}/clone`

**输入**:
```json
{
  "new_name": "string"  // 必需
}
```

**行为**:
- 克隆项目及其环境、集合
- 重写所有 ID 映射
- 保持默认环境关系

### 1.6 environments - 环境 CRUD

#### 1.6.1 List Environments
**端点**: `GET /environments?q={query}&project_id={id}`

#### 1.6.2 Create/Update Environment
**端点**: `POST /environments`

**输入**:
```json
{
  "id": "string",
  "project_id": "string",
  "name": "string",
  "base_url": "string",
  "variables": {"key": "value"},
  "headers": {"key": "value"},
  "tls_config": {
    "enabled": true,
    "ca_file": "string",
    "cert_file": "string",
    "key_file": "string",
    "server_name": "string",
    "insecure": false
  },
  "is_default": false,
  "created_at": "RFC3339",
  "updated_at": "RFC3339"
}
```

#### 1.6.3 Get Environment
**端点**: `GET /environments/{id}`

#### 1.6.4 Delete Environment
**端点**: `DELETE /environments/{id}`

#### 1.6.5 Test Environment
**端点**: `POST /environments/{id}/test`

**输出**:
```json
{
  "success": true,
  "environment": "string",
  "base_url": "string",
  "message": "string",
  "timestamp": 1234567890,
  "tls_enabled": true,
  "tls_server_name": "string",
  "error": "string"
}
```

#### 1.6.6 Set Default Environment
**端点**: `PUT /projects/{id}/default-environment`

**输入**:
```json
{
  "environment_id": "string"
}
```

### 1.7 collections - 集合 CRUD

#### 1.7.1 List Collections
**端点**: `GET /collections?project_id={id}`

#### 1.7.2 Create/Update Collection
**端点**: `POST /collections`

**输入**:
```json
{
  "id": "string",
  "project_id": "string",
  "name": "string",
  "folders": [
    {
      "id": "string",
      "name": "string",
      "items": [RequestItem]
    }
  ],
  "items": [RequestItem],
  "created_at": "RFC3339",
  "updated_at": "RFC3339"
}
```

**RequestItem 结构**:
```json
{
  "id": "string",
  "name": "string",
  "type": "string",
  "service": "string",
  "method": "string",
  "body": "string",
  "metadata": {"key": "value"},
  "env_ref_type": "string",
  "environment_id": "string"
}
```

#### 1.7.3 Import Collections
**端点**: `POST /collections/import`

**输入**:
```json
{
  "data": "JSON string"
}
```

#### 1.7.4 Export Collections
**端点**: `GET /collections/export`

**输出**: 文件下载，Content-Disposition: attachment

### 1.8 history - 历史记录 CRUD

#### 1.8.1 List History
**端点**: `GET /history?limit={n}&offset={n}`

**默认值**: limit=100, offset=0

#### 1.8.2 Search History
**端点**: `GET /history/search?q={query}&service={s}&method={m}&status={s}&start_time={ts}&end_time={ts}&limit={n}&offset={n}`

#### 1.8.3 Add History
**端点**: `POST /history`

**输入**:
```json
{
  "id": "string",
  "project_id": "string",
  "timestamp": 1234567890,
  "service": "string",
  "method": "string",
  "address": "string",
  "status": "string",
  "duration": 123,
  "request_snapshot": RequestItem
}
```

#### 1.8.4 Clear History
**端点**: `DELETE /history`

**输出**: 204 No Content

## 2. 接口语义 (Interface Semantics)

### 2.1 请求体通用规则

1. **JSON 解析**: 所有 POST/PUT 请求体必须是合法 JSON
2. **字段缺失**: 可选字段缺失时使用默认值
3. **时间格式**: RFC3339 格式字符串 (Go: time.RFC3339)
4. **ID 生成**: 服务端自动生成 32 位十六进制随机字符串

### 2.2 响应格式

**成功响应**:
- 200 OK: 查询操作
- 201 Created: 创建操作
- 204 No Content: 删除操作

**错误响应格式**:
```json
{
  "error": "error message"
}
```

**Content-Type**: `application/json`

### 2.3 Metadata 处理

- gRPC metadata 以普通 JSON 对象传递
- 键值对均为字符串类型
- 支持多值（以逗号分隔存储）

### 2.4 TLS 配置结构

```go
type TLSConfig struct {
    Enabled    bool   `json:"enabled"`
    CAFile     string `json:"ca_file,omitempty"`
    CertFile   string `json:"cert_file,omitempty"`
    KeyFile    string `json:"key_file,omitempty"`
    ServerName string `json:"server_name,omitempty"`
    Insecure   bool   `json:"insecure"`
}
```

### 2.5 流式响应 SSE 格式

```
Content-Type: text/event-stream
Cache-Control: no-cache
Connection: keep-alive

data: {"type": "message", "data": {...}}

data: {"type": "end", "status": "OK"}
```

## 3. 迁移完成检查清单

### 3.1 功能等价性检查

- [ ] **connect**: 建立 gRPC 连接，加载 proto/反射，返回连接状态
- [ ] **services**: 返回服务列表和方法信息
- [ ] **invoke**: 一元调用成功/失败处理，metadata 传递，duration 计算
- [ ] **server-stream**: SSE 输出，message/end/error 事件类型
- [ ] **client-stream**: 多消息发送，单次响应
- [ ] **bidi-stream**: SSE 输出，双向消息处理
- [ ] **projects CRUD**: 完整 CRUD，克隆功能
- [ ] **environments CRUD**: 完整 CRUD，测试连接，默认环境设置
- [ ] **collections CRUD**: 完整 CRUD，导入导出
- [ ] **history CRUD**: 完整 CRUD，搜索过滤

### 3.2 数据结构等价性

- [ ] Project 结构完全匹配
- [ ] Environment 结构完全匹配（含 TLSConfig）
- [ ] Collection 结构完全匹配（含 Folder, RequestItem）
- [ ] History 结构完全匹配
- [ ] Service/Method 信息结构完全匹配

### 3.3 错误处理等价性

- [ ] 400 Bad Request: 请求解析错误，验证失败
- [ ] 404 Not Found: 资源不存在
- [ ] 405 Method Not Allowed: 方法不支持
- [ ] 500 Internal Server Error: 服务器内部错误

### 3.4 边界行为等价性

- [ ] 连接超时处理（10秒）
- [ ] 客户端流超时（30秒）
- [ ] 空请求体处理（转为 {}）
- [ ] 字符串形式的 JSON body 处理
- [ ] 反射失败不中断连接
- [ ] 项目删除级联行为

## 4. 关键回归测试用例

### 4.1 connect 测试

| 用例 | 输入 | 期望结果 |
|------|------|----------|
| TC-CONN-01 | 有效地址，无 TLS | 连接成功，connected=true |
| TC-CONN-02 | 无效地址 | 返回错误，状态码 500 |
| TC-CONN-03 | 有效地址，proto 文件 | has_proto=true, proto_source=file |
| TC-CONN-04 | 有效地址，使用反射 | has_proto=true, proto_source=reflection |
| TC-CONN-05 | 反射不可用 | has_proto=false，不报错 |
| TC-CONN-06 | 已有连接时新建 | 旧连接关闭，新连接建立 |

### 4.2 invoke 测试

| 用例 | 输入 | 期望结果 |
|------|------|----------|
| TC-INV-01 | 有效服务/方法，有效 body | 返回 data, metadata, duration, status=OK |
| TC-INV-02 | 未连接调用 | 400 错误 |
| TC-INV-03 | 无效服务名 | 400 错误，获取 input type 失败 |
| TC-INV-04 | 无效 body JSON | 400 错误，解析失败 |
| TC-INV-05 | gRPC 错误响应 | 500 错误，status=ERROR，包含错误信息 |
| TC-INV-06 | 带 metadata 调用 | metadata 正确传递到服务端 |

### 4.3 stream 测试

| 用例 | 输入 | 期望结果 |
|------|------|----------|
| TC-STR-01 | Server stream，有效请求 | SSE 流，包含多条 message + end |
| TC-STR-02 | Client stream，多消息 | 发送所有消息，返回单次响应 |
| TC-STR-03 | Bidi stream，多消息 | SSE 流，双向消息正确 |
| TC-STR-04 | 流中断 | 正确返回 error 事件 |
| TC-STR-05 | 未连接调用 | 400 错误 |

### 4.4 projects 测试

| 用例 | 输入 | 期望结果 |
|------|------|----------|
| TC-PROJ-01 | 创建项目，有效 name | 201，返回项目对象，ID 自动生成 |
| TC-PROJ-02 | 创建项目，空 name | 400 错误 |
| TC-PROJ-03 | 获取项目列表 | 返回所有项目，按 name 排序 |
| TC-PROJ-04 | 获取单个项目 | 返回项目详情 |
| TC-PROJ-05 | 获取不存在的项目 | 404 错误 |
| TC-PROJ-06 | 更新项目 | 部分更新成功，updated_at 更新 |
| TC-PROJ-07 | 删除项目 | 204，级联删除关联数据 |
| TC-PROJ-08 | 克隆项目 | 新项目，新 ID，关联数据克隆 |

### 4.5 environments 测试

| 用例 | 输入 | 期望结果 |
|------|------|----------|
| TC-ENV-01 | 创建环境，完整数据 | 201，返回环境对象 |
| TC-ENV-02 | 按项目查询环境 | 返回项目下环境，默认优先 |
| TC-ENV-03 | 设置默认环境 | 事务内更新所有相关表 |
| TC-ENV-04 | 测试环境连接 | 返回测试结果对象 |
| TC-ENV-05 | 删除环境 | 204，清理关联关系 |

### 4.6 collections 测试

| 用例 | 输入 | 期望结果 |
|------|------|----------|
| TC-COL-01 | 创建集合，完整数据 | 201，返回集合对象 |
| TC-COL-02 | 导入集合 | 201，数据正确解析存储 |
| TC-COL-03 | 导出集合 | 文件下载，格式正确 |
| TC-COL-04 | 按项目查询集合 | 返回项目下集合 |

### 4.7 history 测试

| 用例 | 输入 | 期望结果 |
|------|------|----------|
| TC-HIST-01 | 添加历史记录 | 201，数据正确存储 |
| TC-HIST-02 | 查询历史，分页 | 返回指定数量记录 |
| TC-HIST-03 | 搜索历史，过滤条件 | 返回匹配记录 |
| TC-HIST-04 | 清空历史 | 204，所有记录删除 |

## 5. 关键文件路径

### 5.1 Go Sidecar
- `/Users/jacky/Codehub/grpcui/sidecar/api/routes.go` - HTTP 路由和处理器
- `/Users/jacky/Codehub/grpcui/sidecar/api/project_handlers.go` - 项目相关处理器
- `/Users/jacky/Codehub/grpcui/sidecar/internal/grpc/client.go` - gRPC 客户端
- `/Users/jacky/Codehub/grpcui/sidecar/internal/proto/parser.go` - Proto 解析器
- `/Users/jacky/Codehub/grpcui/sidecar/internal/storage/store.go` - 数据库存储
- `/Users/jacky/Codehub/grpcui/sidecar/internal/storage/project_store.go` - 项目存储
- `/Users/jacky/Codehub/grpcui/sidecar/internal/storage/models.go` - 数据模型
- `/Users/jacky/Codehub/grpcui/sidecar/internal/tls/manager.go` - TLS 管理

### 5.2 Rust Tauri
- `/Users/jacky/Codehub/grpcui/src/src-tauri/src/commands.rs` - Tauri 命令
- `/Users/jacky/Codehub/grpcui/src/src-tauri/src/sidecar.rs` - Sidecar 管理

## 6. 数据结构参考

### 6.1 Go 模型 (sidecar/internal/storage/models.go)

```go
type Project struct {
    ID                   string    `json:"id"`
    Name                 string    `json:"name"`
    Description          string    `json:"description"`
    DefaultEnvironmentID string    `json:"default_environment_id,omitempty"`
    ProtoFiles           []string  `json:"proto_files,omitempty"`
    CreatedAt            time.Time `json:"created_at"`
    UpdatedAt            time.Time `json:"updated_at"`
}

type Environment struct {
    ID        string            `json:"id"`
    ProjectID string            `json:"project_id"`
    Name      string            `json:"name"`
    BaseURL   string            `json:"base_url"`
    Variables map[string]string `json:"variables"`
    Headers   map[string]string `json:"headers"`
    TLSConfig *TLSConfig        `json:"tls_config,omitempty"`
    IsDefault bool              `json:"is_default"`
    CreatedAt time.Time         `json:"created_at"`
    UpdatedAt time.Time         `json:"updated_at"`
}

type TLSConfig struct {
    Enabled    bool   `json:"enabled"`
    CAFile     string `json:"ca_file,omitempty"`
    CertFile   string `json:"cert_file,omitempty"`
    KeyFile    string `json:"key_file,omitempty"`
    ServerName string `json:"server_name,omitempty"`
    Insecure   bool   `json:"insecure"`
}

type Collection struct {
    ID        string        `json:"id"`
    ProjectID string        `json:"project_id"`
    Name      string        `json:"name"`
    Folders   []Folder      `json:"folders"`
    Items     []RequestItem `json:"items"`
    CreatedAt time.Time     `json:"created_at"`
    UpdatedAt time.Time     `json:"updated_at"`
}

type Folder struct {
    ID    string        `json:"id"`
    Name  string        `json:"name"`
    Items []RequestItem `json:"items"`
}

type RequestItem struct {
    ID            string            `json:"id"`
    Name          string            `json:"name"`
    Type          string            `json:"type"`
    Service       string            `json:"service"`
    Method        string            `json:"method"`
    Body          string            `json:"body"`
    Metadata      map[string]string `json:"metadata"`
    EnvRefType    string            `json:"env_ref_type"`
    EnvironmentID string            `json:"environment_id,omitempty"`
}

type History struct {
    ID              string      `json:"id"`
    ProjectID       string      `json:"project_id,omitempty"`
    Timestamp       int64       `json:"timestamp"`
    Service         string      `json:"service"`
    Method          string      `json:"method"`
    Address         string      `json:"address"`
    Status          string      `json:"status"`
    Duration        int64       `json:"duration"`
    RequestSnapshot RequestItem `json:"request_snapshot"`
}
```

### 6.2 Rust 结构 (src/src-tauri/src/commands.rs)

```rust
pub struct ConnectRequest {
    address: String,
    tls: Option<TLSConfig>,
    insecure: bool,
    proto_file: Option<String>,
    use_reflection: bool,
}

pub struct TLSConfig {
    mode: String,
    ca_cert: Option<String>,
    client_cert: Option<String>,
    client_key: Option<String>,
    skip_verify: Option<bool>,
}

pub struct ConnectResponse {
    success: bool,
    error: Option<String>,
}

pub struct Service {
    name: String,
    full_name: String,
    methods: Vec<Method>,
}

pub struct Method {
    name: String,
    full_name: String,
    input_type: String,
    output_type: String,
    r#type: String,
}

pub struct InvokeRequest {
    method: String,
    body: String,
    metadata: Option<HashMap<String, String>>,
}

pub struct InvokeResponse {
    data: Option<serde_json::Value>,
    error: Option<String>,
    metadata: HashMap<String, String>,
    duration: u64,
    status: String,
}

pub struct StreamInvokeRequest {
    method: String,
    body: String,
    metadata: Option<HashMap<String, String>>,
    stream_type: String, // "client", "server", "bidi"
}

pub struct Project {
    id: String,
    name: String,
    description: String,
    default_environment_id: Option<String>,
    proto_files: Option<Vec<String>>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

pub struct Environment {
    id: String,
    project_id: Option<String>,
    name: String,
    base_url: String,
    variables: HashMap<String, String>,
    headers: HashMap<String, String>,
    tls_config: Option<serde_json::Value>,
    is_default: bool,
    created_at: Option<String>,
    updated_at: Option<String>,
}

pub struct Collection {
    id: String,
    project_id: Option<String>,
    name: String,
    folders: Vec<Folder>,
    items: Vec<RequestItem>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

pub struct Folder {
    id: String,
    name: String,
    items: Vec<RequestItem>,
}

pub struct RequestItem {
    id: String,
    name: String,
    r#type: String,
    service: String,
    method: String,
    body: String,
    metadata: HashMap<String, String>,
    env_ref_type: Option<String>,
    environment_id: Option<String>,
}

pub struct History {
    id: String,
    project_id: Option<String>,
    timestamp: u64,
    service: String,
    method: String,
    address: String,
    status: String,
    duration: u64,
    request_snapshot: RequestItem,
}
```

---

**文档版本**: M0-BASELINE-001
**生成日期**: 2026-02-09
**适用范围**: gRPC UI 迁移项目基线冻结
