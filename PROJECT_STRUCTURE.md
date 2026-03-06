# gRPC UI - 项目结构

```
grpcui/
├── README.md                   # 项目说明
├── PROJECT_STRUCTURE.md        # 本文件
├── docs/
│   ├── grpc-gui-prd.md         # 产品需求文档
│   ├── tech-architecture.md    # 技术架构设计
│   └── implementation-plan.md  # 实施计划
│
├── scripts/
│   ├── dev.sh                  # 开发启动脚本
│   └── build.sh                # 构建脚本
│
├── src/                        # Tauri + 前端
│   ├── package.json            # 前端依赖
│   ├── tsconfig.json           # TypeScript 配置
│   ├── vite.config.ts          # Vite 配置
│   ├── tailwind.config.js      # Tailwind 配置
│   ├── index.html              # 入口 HTML
│   └── src/
│       ├── main.tsx            # React 入口
│       ├── App.tsx             # 主应用组件
│       ├── index.css           # 全局样式
│       ├── lib/
│       │   └── utils.ts        # 工具函数
│       ├── types/
│       │   └── index.ts        # TypeScript 类型定义
│       ├── components/         # UI 组件 (待开发)
│       ├── hooks/              # React Hooks (待开发)
│       └── stores/             # 状态管理 (待开发)
│
│   └── src-tauri/              # Tauri (Rust)
│       ├── Cargo.toml          # Rust 依赖
│       ├── build.rs            # 构建脚本
│       ├── tauri.conf.json     # Tauri 配置
│       └── src/
│           ├── main.rs         # 入口
│           ├── sidecar.rs      # Sidecar 管理
│           └── commands.rs     # 前端命令
│
└── sidecar/                    # Go gRPC 引擎
    ├── go.mod                  # Go 模块
    ├── grpc-sidecar            # 编译后的二进制
    ├── main.go                 # 入口
    ├── cmd/
    │   └── server.go           # 服务器命令
    ├── internal/
    │   ├── storage/
    │   │   ├── store.go        # SQLite 存储
    │   │   └── models.go       # 数据模型
    │   ├── grpc/               # gRPC 客户端 (待开发)
    │   ├── proto/              # Proto 解析器 (待开发)
    │   ├── env/                # 环境变量解析 (待开发)
    │   └── tls/                # TLS 管理 (待开发)
    └── api/
        └── routes.go           # HTTP 路由
```

## 技术栈

- **前端**: React 18 + TypeScript + Tailwind CSS + Monaco Editor
- **桌面框架**: Tauri 2.x (Rust)
- **gRPC 引擎**: Go + sqlite3
- **通信**: HTTP (Tauri ↔ Go Sidecar)

## 运行项目

```bash
# 开发模式
./scripts/dev.sh

# 构建
./scripts/build.sh
```

## 已完成

✅ 项目架构设计
✅ Tauri + React 前端框架
✅ Go Sidecar 基础架构
✅ SQLite 存储层
✅ HTTP API 路由
✅ 基础 UI 布局

## 待开发

🚧 gRPC 客户端实现
🚧 Proto 解析器
🚧 流式调用支持
🚧 前端组件完善
🚧 环境变量系统
🚧 收藏和历史功能
