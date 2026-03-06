# 技术架构设计

## 系统架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                      GRPC GUI (Tauri App)                       │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                   Frontend (React + TS)                   │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌──────────────────┐  │  │
│  │  │  Request    │  │  Response   │  │   Service Tree   │  │  │
│  │  │   Panel     │  │   Panel     │  │    Explorer      │  │  │
│  │  └─────────────┘  └─────────────┘  └──────────────────┘  │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌──────────────────┐  │  │
│  │  │ Environment │  │ Collection  │  │   History List   │  │  │
│  │  │   Manager   │  │   Manager   │  │                  │  │  │
│  │  └─────────────┘  └─────────────┘  └──────────────────┘  │  │
│  └──────────────────────────────────────────────────────────┘  │
│                              │                                  │
│                    Tauri Bridge (IPC)                           │
│                              │                                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │              Go Sidecar (gRPC Engine)                     │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌──────────────────┐  │  │
│  │  │   gRPC      │  │    Proto    │  │   TLS Manager    │  │  │
│  │  │   Client    │  │   Parser    │  │                  │  │  │
│  │  └─────────────┘  └─────────────┘  └──────────────────┘  │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌──────────────────┐  │  │
│  │  │  Streaming  │  │   Storage   │  │   Environment    │  │  │
│  │  │   Handler   │  │   (SQLite)  │  │    Resolver      │  │  │
│  │  └─────────────┘  └─────────────┘  └──────────────────┘  │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## 组件说明

### Frontend (React + TypeScript)

**职责**:
- 用户界面渲染
- 状态管理
- 与 Tauri 后端通信

**核心模块**:
1. **RequestPanel** - 请求构造面板
   - 服务/方法选择
   - JSON 编辑器（请求体）
   - Metadata 编辑
   - 环境变量注入

2. **ResponsePanel** - 响应展示面板
   - 响应状态显示
   - JSON 格式化展示
   - 流式消息列表
   - 耗时统计

3. **ServiceExplorer** - 服务浏览器
   - 服务列表树
   - 方法详情
   - 消息类型定义

4. **EnvironmentManager** - 环境管理
   - 环境 CRUD
   - 变量编辑器
   - 环境切换

5. **CollectionManager** - 收藏管理
   - 收藏夹目录树
   - 导入/导出

6. **HistoryList** - 历史记录
   - 时间线展示
   - 快速恢复

### Tauri (Rust)

**职责**:
- 应用生命周期管理
- 系统菜单、托盘
- 文件系统访问（读 proto 文件）
- Go Sidecar 管理（启动、停止、通信）

**核心功能**:
- 启动时启动 Go Sidecar
- 通过 stdin/stdout 或 HTTP 与 Sidecar 通信
- 前端 API 封装

### Go Sidecar

**职责**:
- 所有 gRPC 逻辑
- 协议解析
- 数据持久化

**核心模块**:

1. **gRPC Client** (`grpc/client.go`)
   ```go
   type Client interface {
       Connect(addr string, opts ConnectOptions) error
       ListServices() ([]Service, error)
       Invoke(method string, body []byte, md metadata.MD) (*Response, error)
       InvokeServerStream(method string, body []byte, md metadata.MD) (Stream, error)
       InvokeClientStream(method string, md metadata.MD) (Stream, error)
       InvokeBidiStream(method string, md metadata.MD) (Stream, error)
   }
   ```

2. **Proto Parser** (`proto/parser.go`)
   ```go
   type Parser interface {
       ParseFromReflection(ctx context.Context, addr string) (*DescriptorSet, error)
       ParseFromFiles(files []string, importPaths []string) (*DescriptorSet, error)
   }
   ```

3. **Storage** (`storage/store.go`)
   ```go
   type Store interface {
       // Environment
       SaveEnvironment(env *Environment) error
       GetEnvironments() ([]Environment, error)
       
       // Collection
       SaveCollection(col *Collection) error
       GetCollections() ([]Collection, error)
       
       // History
       AddHistory(h *History) error
       GetHistories(limit int) ([]History, error)
   }
   ```

4. **Environment Resolver** (`env/resolver.go`)
   - 变量替换 `{{variable}}`
   - 全局变量管理

5. **TLS Manager** (`tls/manager.go`)
   - 证书加载
   - TLS 配置生成

## 通信协议

### Frontend ↔ Tauri
使用 Tauri 的 Command 和 Event API:
```typescript
// 调用 Sidecar 命令
invoke('grpc_call', { 
  address: 'localhost:50051',
  method: 'Greeter.SayHello',
  body: '{"name": "World"}',
  metadata: { 'authorization': 'Bearer xxx' }
})

// 监听流式响应
listen('grpc_stream_message', (event) => {
  console.log('Received:', event.payload)
})
```

### Tauri ↔ Go Sidecar
选择方案 **HTTP Localhost**:
- Sidecar 启动 HTTP server 在 `127.0.0.1:0`（随机端口）
- Tauri 通过 stdin 获取端口号
- 后续通信通过 HTTP REST API

**原因**:
- 简单直观，易于调试
- 天然支持流式响应（SSE）
- 无需处理复杂的 stdin/stdout 协议

**API 定义**:
```go
// POST /connect
{"address": "localhost:50051", "tls": {...}}

// GET /services
// Response: {"services": [...]}

// POST /invoke
{"method": "Greeter.SayHello", "body": "...", "metadata": {...}}
// Response: {"response": "...", "metadata": {...}, "duration_ms": 42}

// POST /invoke/server-stream
// Response: SSE stream

event: message
data: {"type": "message", "payload": "..."}

event: error
data: {"type": "error", "message": "..."}

event: end
data: {"type": "end", "stats": {...}}
```

## 数据模型

### Environment
```typescript
interface Environment {
  id: string;
  name: string;           // "Development", "Staging", "Production"
  baseUrl: string;        // "localhost:50051"
  tls: TLSConfig;
  metadata: Record<string, string>;
  variables: Variable[];
}

interface Variable {
  key: string;
  value: string;
  secret: boolean;        // 敏感信息标记
}

interface TLSConfig {
  mode: 'insecure' | 'system' | 'custom';
  caCert?: string;        // PEM 内容
  clientCert?: string;
  clientKey?: string;
  skipVerify?: boolean;
}
```

### Collection
```typescript
interface Collection {
  id: string;
  name: string;
  folders: Folder[];
  items: RequestItem[];
}

interface Folder {
  id: string;
  name: string;
  items: RequestItem[];
}

interface RequestItem {
  id: string;
  name: string;
  type: 'unary' | 'server_stream' | 'client_stream' | 'bidi_stream';
  service: string;
  method: string;
  body: string;
  metadata: Record<string, string>;
  environmentId?: string;
}
```

### History
```typescript
interface History {
  id: string;
  timestamp: number;
  service: string;
  method: string;
  address: string;
  status: 'success' | 'error';
  duration: number;
  requestSnapshot: RequestItem;
}
```

## 项目结构

```
grpcui/
├── README.md
├── docs/
│   ├── grpc-gui-prd.md
│   └── tech-architecture.md
│
├── src/                          # Tauri + Frontend
│   ├── src-tauri/               # Rust Tauri 代码
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── main.rs          # 入口
│   │   │   ├── sidecar.rs       # Go sidecar 管理
│   │   │   └── commands.rs      # 前端命令
│   │   └── tauri.conf.json
│   │
│   ├── package.json             # 前端依赖
│   ├── src/                     # React 源码
│   │   ├── main.tsx
│   │   ├── App.tsx
│   │   ├── components/
│   │   │   ├── RequestPanel.tsx
│   │   │   ├── ResponsePanel.tsx
│   │   │   ├── ServiceTree.tsx
│   │   │   ├── EnvironmentEditor.tsx
│   │   │   ├── CollectionTree.tsx
│   │   │   ├── HistoryList.tsx
│   │   │   └── JsonEditor.tsx
│   │   ├── hooks/
│   │   │   ├── useGrpc.ts
│   │   │   ├── useStorage.ts
│   │   │   └── useEnvironment.ts
│   │   ├── stores/
│   │   │   ├── appStore.ts
│   │   │   └── requestStore.ts
│   │   └── types/
│   │       └── index.ts
│   └── index.html
│
├── sidecar/                     # Go Sidecar
│   ├── go.mod
│   ├── main.go
│   ├── cmd/
│   │   └── server.go
│   ├── internal/
│   │   ├── grpc/
│   │   │   ├── client.go
│   │   │   ├── stream.go
│   │   │   └── types.go
│   │   ├── proto/
│   │   │   ├── parser.go
│   │   │   └── descriptor.go
│   │   ├── storage/
│   │   │   ├── store.go
│   │   │   ├── sqlite.go
│   │   │   └── models.go
│   │   ├── env/
│   │   │   └── resolver.go
│   │   └── tls/
│   │       └── manager.go
│   └── api/
│       ├── handler.go
│       ├── routes.go
│       └── middleware.go
│
├── scripts/
│   └── build.sh
└── .gitignore
```

## 技术选型理由

### 为什么选 Tauri + Go Sidecar？

| 方案 | 优点 | 缺点 |
|------|------|------|
| **Wails (Go)** | 纯 Go 栈，简单统一 | 生态不如 Tauri 成熟，前端集成度稍弱 |
| **Tauri + Go** | Tauri 生态好，Go 处理 gRPC 成熟 | 需要管理 Sidecar 进程 |
| **Tauri + Rust** | 统一 Rust 栈，性能好 | gRPC 生态不如 Go 成熟，proto 解析复杂 |
| **Electron + Node** | 生态最成熟 | 包体积大，内存占用高 |

**最终选择 Tauri + Go Sidecar**:
1. Go 的 gRPC 生态最成熟（官方库、protoreflect）
2. Tauri 比 Electron 包体积小 10 倍以上
3. Sidecar 模式可独立升级 gRPC 引擎

### 前端技术栈
- **React 18**: 成熟稳定，生态丰富
- **TypeScript**: 类型安全
- **shadcn/ui**: 现代美观的组件库，可定制性强
- **Monaco Editor**: VS Code 同款编辑器，JSON 支持好
- **Zustand**: 简洁的状态管理
- **TanStack Query**: 服务端状态管理（Sidecar 通信）

## 构建流程

```bash
# 1. 构建 Go Sidecar
cd sidecar
go build -o ../src/src-tauri/binaries/grpc-sidecar

# 2. 构建 Tauri 应用
cd ../src
npm install
npm run tauri build

# 输出
# src/src-tauri/target/release/GrpcUI.app (macOS)
# src/src-tauri/target/release/GrpcUI.exe (Windows)
```

## 开发环境启动

```bash
# 终端 1: 启动 Go Sidecar
cd sidecar
go run main.go server --port 9845

# 终端 2: 启动前端开发服务器
cd src
npm install
npm run dev

# Tauri 开发模式（会自动启动 Sidecar）
npm run tauri dev
```
