# 实施计划

## Phase 1: 基础设施搭建 (Week 1)

### Day 1-2: 项目初始化
- [ ] 创建项目目录结构
- [ ] 初始化 Tauri 项目
- [ ] 初始化 Go Sidecar 模块
- [ ] 配置开发环境

### Day 3-4: Sidecar 基础架构
- [ ] 搭建 Go HTTP 服务框架
- [ ] 实现 Tauri 与 Sidecar 通信机制
- [ ] 基础 API 路由
- [ ] 错误处理和日志

### Day 5-7: 存储层
- [ ] SQLite 数据库初始化
- [ ] Environment 存储接口
- [ ] Collection 存储接口
- [ ] History 存储接口

## Phase 2: gRPC 核心功能 (Week 2-3)

### Week 2
- [ ] Proto 文件解析器
  - [ ] 从文件解析 proto
  - [ ] 处理 import 依赖
  - [ ] 生成服务/方法描述
- [ ] gRPC Reflection 支持
  - [ ] 连接并获取服务列表
  - [ ] 解析方法定义
- [ ] gRPC Client 基础
  - [ ] 连接管理
  - [ ] Unary 调用实现

### Week 3
- [ ] Streaming 支持
  - [ ] Server Streaming
  - [ ] Client Streaming
  - [ ] Bidirectional Streaming
- [ ] TLS 管理
  - [ ] 证书加载
  - [ ] TLS 配置
- [ ] Metadata 处理
- [ ] 环境变量解析器

## Phase 3: 前端核心功能 (Week 4-5)

### Week 4
- [ ] 前端框架搭建
  - [ ] React + TypeScript 配置
  - [ ] 组件库集成
  - [ ] 状态管理配置
- [ ] 基础布局
  - [ ] 侧边栏（服务树、收藏、环境）
  - [ ] 主区域（请求/响应面板）
- [ ] Service Explorer
  - [ ] 服务列表展示
  - [ ] 方法选择

### Week 5
- [ ] Request Panel
  - [ ] JSON 编辑器集成
  - [ ] Metadata 编辑
  - [ ] 环境变量注入
- [ ] Response Panel
  - [ ] JSON 格式化显示
  - [ ] 流式消息展示
  - [ ] 状态码和耗时
- [ ] 请求发送逻辑

## Phase 4: 高级功能 (Week 6)

- [ ] Environment Manager UI
  - [ ] 环境 CRUD
  - [ ] 变量编辑器
- [ ] Collection Manager UI
  - [ ] 收藏夹目录树
  - [ ] 保存请求
- [ ] History List UI
  - [ ] 历史记录展示
  - [ ] 快速恢复
- [ ] 导入/导出功能

## Phase 5: 完善与发布 (Week 7)

- [ ] 错误处理优化
- [ ] 加载状态
- [ ] UI 细节打磨
- [ ] 主题切换
- [ ] 跨平台测试
- [ ] 构建配置
- [ ] README 文档

## 关键里程碑

| 日期 | 里程碑 | 可演示功能 |
|------|--------|-----------|
| Week 1 结束 | 基础架构完成 | Sidecar 启动，基础 API 可用 |
| Week 2 结束 | Proto 解析完成 | 可解析 proto，列出服务方法 |
| Week 3 结束 | gRPC 调用完成 | 可执行所有类型调用 |
| Week 5 结束 | MVP 功能完成 | 完整可用，支持环境、收藏、历史 |
| Week 7 结束 | v1.0 发布 | 可公开发布 |

## 风险与应对

| 风险 | 可能性 | 应对策略 |
|------|--------|----------|
| Proto 解析复杂 | 中 | 使用成熟库，预留缓冲时间 |
| Streaming UI 卡顿 | 中 | Week 5 专门优化，虚拟列表 |
| Tauri 构建问题 | 低 | 早期验证构建流程 |
