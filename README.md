# gRPC UI

一个基于 Tauri 2 的桌面 gRPC 调试工具，当前实现为：

- 前端：React + TypeScript + Vite + Zustand + i18next
- 桌面壳：Tauri (Rust)
- gRPC 描述与编解码：Go FFI 动态库
- gRPC 传输与调用：Rust (tonic + hyper)
- 本地数据：SQLite (Rust 侧维护)

## 当前已实现能力

### 1) 连接与服务发现

- 支持通过 gRPC Reflection 连接目标服务
- 支持导入单个 `.proto` 文件
- 支持导入 `.proto` 目录（递归扫描 + 导入预览）
- 支持拖拽文件/目录到导入弹窗
- 项目切换时自动恢复该项目保存的 proto 服务树

### 2) 调用能力

- 支持 Unary / Server Streaming / Client Streaming / Bidi Streaming
- 请求体支持两种模式：
  - JSON 编辑
  - 基于方法入参 schema 的结构化编辑
- 支持请求 Metadata 编辑
- 支持响应状态、耗时、Metadata 展示
- 流式调用支持发送消息、半关闭输入、主动关闭流

### 3) 项目与数据管理

- 项目：创建、克隆、删除、切换
- 环境：baseUrl、变量、请求头、TLS 配置
- 请求级环境策略：继承默认环境 / 指定环境 / 不使用环境
- 收藏：保存请求并在集合中复用
- 历史：按项目记录调用历史，支持单条删除与清空
- 请求发送前支持 `{{var}}` 变量替换（地址、请求体、metadata）

### 4) 界面与体验

- 中英文切换（i18next）
- 主题模式：跟随系统 / 深色 / 浅色

## 架构概览

```
React UI
  │  (Tauri invoke)
  ▼
Rust Commands (src-tauri)
  ├─ Storage: SQLite (projects/environments/collections/history)
  ├─ gRPC Transport: tonic/hyper (unary + streaming)
  └─ Go FFI Bridge: proto 解析 + JSON<->Protobuf 编解码
           │
           ▼
      gRPC Server
```

## 快速开始

### 前置环境

- Bun 1.x（开发模式默认使用）
- Node.js 20+
- Go 1.23+（需可用 CGO 与 C 编译环境）
- Rust stable + Cargo
- Linux 额外依赖（参考 CI）：`libgtk-3-dev libwebkit2gtk-4.0-dev libappindicator3-dev librsvg2-dev patchelf`

### 一键开发启动（推荐）

在仓库根目录执行：

```bash
./scripts/dev.sh
```

这个脚本会自动完成：

1. 按平台构建/复用 FFI 动态库
2. 设置 `GRPC_CODEC_BRIDGE_PATH`
3. 将开发数据库放到系统临时目录（避免 watcher 重启风暴）
4. 启动 `tauri:dev`

### 手动启动（调试脚本时使用）

```bash
# 1) 构建 FFI 库
./scripts/build-ffi.sh

# 2) 设置 FFI 动态库路径（按平台选择）
# macOS
export GRPC_CODEC_BRIDGE_PATH="$(pwd)/src/src-tauri/resources/libgrpc_codec_bridge.dylib"
# Linux
# export GRPC_CODEC_BRIDGE_PATH="$(pwd)/src/src-tauri/resources/libgrpc_codec_bridge.so"

# 3) (可选) 指定数据库位置
export GRPCUI_DB_PATH="${TMPDIR:-/tmp}/grpcui-dev/grpcui.db"

# 4) 启动应用
cd src
bun install
bun run tauri:dev
```

### 本地联调示例服务（可选）

```bash
cd tests/mocks
go run echo-server.go
```

然后在 UI 中连接 `localhost:50051`（可用 reflection），或导入 `tests/mocks/echo.proto`。

## 构建发布

### 构建当前平台

```bash
./scripts/build.sh
```

### 构建指定平台

```bash
./scripts/build.sh --target darwin-arm64
./scripts/build.sh --target darwin-x64
./scripts/build.sh --target linux-x64
./scripts/build.sh --target linux-arm64
./scripts/build.sh --target windows-x64
```

### 仅构建 FFI 动态库

```bash
./scripts/build-ffi.sh
./scripts/build-ffi.sh --target darwin-arm64
```

构建产物输出在 `build/` 目录。

## 开发与质量检查

### 前端

```bash
cd src
bun run lint
bun run test
bun run test:e2e
```

### Rust (Tauri)

```bash
cd src/src-tauri
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test
```

### Go (FFI)

```bash
cd sidecar
go mod tidy
golangci-lint run
go test ./...
```

## 目录结构（实际）

```
grpcui/
├── src/
│   ├── src/                   # React 前端
│   ├── src-tauri/             # Tauri + Rust 命令层
│   └── tests/                 # 前端单测/E2E 与 mock
├── sidecar/
│   ├── ffi/                   # Go FFI 导出层（c-shared）
│   └── internal/proto/        # Proto 解析与 schema 提取
├── scripts/                   # dev/build 脚本
├── docs/                      # 设计与计划文档
└── .github/workflows/ci.yml   # CI/CD 流水线
```

## 环境变量

- `GRPC_CODEC_BRIDGE_PATH`：指定 Go FFI 动态库路径
- `GRPCUI_DB_PATH`：指定 SQLite 数据库路径
- `GRPC_DEBUG`：开启 gRPC 调用调试日志（Rust 侧）
- `TAURI_DEV_WATCHER_IGNORE_FILE`：开发 watcher 忽略规则（由 `scripts/dev.sh` 设置）

## 当前实现边界

- TLS 配置模型已打通（含 authority、证书路径字段存储）。
- 当前调用链对证书文件路径（自定义 CA / mTLS）的消费仍在完善中，日常建议优先使用：
  - insecure（本地/内网调试）
  - 或系统根证书模式
- 收藏数据结构已支持 folder 字段，但 UI 当前主要以平铺列表方式展示请求。
- `scripts/dev.sh` 当前仅覆盖 macOS / Linux 本机开发流程。

## 相关文档

- [项目环境架构](./docs/project-env-architecture.md)
- [技术架构设计](./docs/tech-architecture.md)
- [实施计划](./docs/implementation-plan.md)

## License

MIT
