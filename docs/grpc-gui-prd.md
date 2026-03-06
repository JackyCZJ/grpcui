# Product Requirements Document: GRPC GUI 测试工具

**Version**: 1.0  
**Date**: 2026-02-09  
**Author**: Sarah (Product Owner)  
**Quality Score**: 90/100

---

## Executive Summary

本项目旨在开发一款现代化的桌面端 gRPC 测试工具，解决开发者在调试 gRPC 服务时缺乏直观、功能完整的 GUI 工具的问题。工具采用 Tauri + Go Sidecar 架构，结合 Web 前端的灵活性与 Go 在 gRPC 领域的成熟生态。

目标用户是后端工程师和全栈开发者，覆盖从本地开发、测试环境验证到生产问题排查的全场景。工具将支持所有四种 gRPC 调用模式、环境管理、请求收藏、TLS 配置等核心功能，成为开发者工具箱中不可或缺的利器。

---

## Problem Statement

**Current Situation**: 
- 现有工具如 Postman 对 gRPC 支持有限或需要付费
- grpcui 是网页版，功能相对基础，不支持环境管理和请求历史
- ezy 功能强大但更新缓慢、处于 Beta 阶段
- 命令行工具如 grpcurl 功能完整但缺乏图形界面，学习成本高

**Proposed Solution**: 
开发一款开源的桌面 gRPC 客户端，提供：
- 直观的图形界面，降低 gRPC 调试门槛
- 完整的调用类型支持（Unary + 三种 Streaming）
- 环境变量管理，轻松切换 dev/staging/prod
- 请求历史与收藏，提高工作效率
- 现代化的技术栈（Tauri + Go），性能优异

**Business Impact**: 
- 个人开发效率提升 30%+（减少命令行操作、快速切换环境）
- 降低团队协作成本（可导出/分享请求配置）
- 成为开源社区有价值的工具贡献

---

## Success Metrics

**Primary KPIs:**
- **功能完整性**: 支持 100% 的 gRPC 调用类型（Unary/Server/Client/Bidirectional）
- **启动速度**: 冷启动时间 < 3 秒
- **响应性能**: 简单 Unary 调用响应时间 < 500ms（不含网络延迟）
- **稳定性**: 连续运行 8 小时无崩溃或内存泄漏

**Validation**: 
- 通过自动化测试验证各调用类型功能正确性
- 使用 Tauri 的性能分析工具测量启动时间
- 长时间运行测试验证稳定性

---

## User Personas

### Primary: 后端开发工程师 Alex
- **Role**: 微服务后端开发
- **Goals**: 
  - 快速调试本地 gRPC 服务
  - 验证测试环境 API 行为
  - 排查生产环境问题
- **Pain Points**: 
  - 命令行工具不够直观
  - 频繁切换环境需要修改多个配置
  - 无法保存常用请求
- **Technical Level**: 高级
- **使用频率**: 每天多次

### Secondary: 全栈开发者 Bob
- **Role**: 全栈工程师，需要与后端服务对接
- **Goals**: 
  - 理解后端接口定义
  - 快速测试接口可用性
- **Pain Points**: 
  - 不熟悉 protobuf 语法
  - 需要可视化查看服务定义
- **Technical Level**: 中级

---

## User Stories & Acceptance Criteria

### Story 1: 发现并连接 gRPC 服务

**As a** 后端开发工程师  
**I want to** 通过服务地址快速发现并连接 gRPC 服务  
**So that** 我无需手动编写 protobuf 文件即可开始测试

**Acceptance Criteria:**
- [ ] 支持 gRPC Reflection 自动获取服务定义
- [ ] 支持手动导入 .proto 文件
- [ ] 支持解析 proto 文件中的 import 依赖
- [ ] 显示服务列表、方法列表及其类型（Unary/Streaming）
- [ ] 连接失败时显示清晰的错误信息

### Story 2: 执行 Unary 调用

**As a** 开发者  
**I want to** 发送简单的 Unary gRPC 请求  
**So that** 我可以测试单请求-单响应的接口

**Acceptance Criteria:**
- [ ] 提供 JSON 编辑器输入请求体，支持语法高亮
- [ ] 支持设置 Metadata/Header
- [ ] 显示响应状态码、Metadata、响应体
- [ ] 响应 JSON 格式化并支持语法高亮
- [ ] 显示请求耗时

### Story 3: 执行 Streaming 调用

**As a** 开发者  
**I want to** 执行 Server/Client/Bidirectional Streaming 调用  
**So that** 我可以测试流式接口

**Acceptance Criteria:**
- [ ] Server Streaming: 实时显示服务端推送的消息流
- [ ] Client Streaming: 支持分多次发送消息，最后接收响应
- [ ] Bidirectional: 支持双向实时消息流
- [ ] 支持手动取消流（Cancel）
- [ ] 流结束后显示统计信息（消息数、耗时）

### Story 4: 管理环境变量

**As a** 开发者  
**I want to** 创建和管理多个环境配置（dev/staging/prod）  
**So that** 我可以快速切换测试目标

**Acceptance Criteria:**
- [ ] 支持创建多个环境配置
- [ ] 每个环境包含：名称、服务端地址、TLS 配置、默认 Metadata
- [ ] 一键切换当前环境
- [ ] 环境变量支持在请求 URL、Metadata、请求体中引用（如 `{{base_url}}`）

### Story 5: 保存和复用请求

**As a** 开发者  
**I want to** 保存常用请求到收藏夹  
**So that** 我可以快速重复执行测试

**Acceptance Criteria:**
- [ ] 支持将当前请求保存到收藏夹，自定义名称
- [ ] 收藏夹按服务/方法组织或支持文件夹分类
- [ ] 点击收藏项自动填充请求参数
- [ ] 支持导入/导出收藏集合（JSON 格式）

### Story 6: 配置 TLS

**As a** 开发者  
**I want to** 配置 TLS 连接（包括自签名证书）  
**So that** 我可以测试 HTTPS 服务

**Acceptance Criteria:**
- [ ] 支持禁用 TLS（insecure）
- [ ] 支持系统默认证书
- [ ] 支持上传自定义 CA 证书
- [ ] 支持 mTLS（客户端证书 + 私钥）
- [ ] 支持跳过证书验证（开发调试用）

### Story 7: 查看请求历史

**As a** 开发者  
**I want to** 查看最近的请求历史  
**So that** 我可以快速回溯之前的测试

**Acceptance Criteria:**
- [ ] 自动记录最近 100 条请求
- [ ] 显示请求时间、服务、方法、状态
- [ ] 点击历史项恢复请求参数
- [ ] 支持清空历史

---

## Functional Requirements

### Core Features

**Feature 1: 服务发现与连接**
- **Description**: 通过反射或 proto 文件发现 gRPC 服务定义
- **User flow**: 
  1. 输入服务地址
  2. 选择发现方式（Reflection / Proto 文件）
  3. 系统加载服务定义并显示服务/方法列表
- **Edge cases**: 
  - 反射未启用时提示用户导入 proto
  - Proto 文件解析错误时显示具体错误位置
- **Error handling**: 网络超时、解析错误、服务不可用

**Feature 2: 请求构造与发送**
- **Description**: 构建并发送 gRPC 请求
- **User flow**: 
  1. 选择服务和 method
  2. 编辑请求体（JSON）
  3. 设置 Metadata
  4. 点击 Send
- **Edge cases**: 
  - JSON 格式错误时阻止发送并高亮错误
  - 大消息体（>1MB）时提供性能提示
- **Error handling**: 网络错误、服务返回错误码、超时

**Feature 3: 流式调用管理**
- **Description**: 管理三种 Streaming 调用模式
- **User flow**: 
  - Server Streaming: Send → 接收流式响应 → Cancel/等待结束
  - Client Streaming: 多次 Send → Close & Receive
  - Bidirectional: 双向实时发送/接收
- **Edge cases**: 
  - 流中途断开连接
  - 大量消息时的渲染性能
- **Error handling**: 流取消、连接中断、服务端错误

**Feature 4: 环境管理系统**
- **Description**: 多环境配置管理
- **User flow**: 
  1. 创建环境（名称、地址、TLS、Metadata）
  2. 在请求中引用环境变量
  3. 快速切换环境
- **Edge cases**: 
  - 变量引用不存在时的提示
  - 环境配置验证
- **Error handling**: 无效的变量格式、循环引用

**Feature 5: 收藏与历史**
- **Description**: 请求持久化和快速恢复
- **User flow**: 
  - 收藏: 点击收藏按钮 → 命名 → 保存到列表
  - 历史: 自动记录 → 点击恢复
- **Edge cases**: 
  - 导入的收藏与环境变量冲突处理
- **Error handling**: 导入格式错误、存储空间不足

**Feature 6: TLS 配置**
- **Description**: 灵活的 TLS 连接配置
- **User flow**: 
  1. 选择 TLS 模式
  2. 上传证书文件（如需要）
  3. 测试连接
- **Edge cases**: 
  - 证书过期提示
  - 证书格式错误
- **Error handling**: 证书加载失败、TLS 握手失败

### Out of Scope
- gRPC-Web 支持（Phase 2 考虑）
- 团队协作/云同步功能
- 自动化测试/断言功能
- Mock 服务
- 性能测试/压力测试
- gRPC Gateway/REST 转译

---

## Technical Constraints

### Performance
- **启动时间**: 冷启动 < 3 秒
- **内存占用**: 正常使用 < 200MB
- **响应渲染**: 大 JSON（>100KB）渲染不卡顿
- **流式消息**: 支持 1000+ 消息流畅显示

### Security
- TLS 证书本地存储，不上传云端
- 敏感信息（token）支持标记为 secret，显示时脱敏
- 不支持收集用户数据

### Integration
- **Tauri**: 桌面应用框架，提供原生窗口、菜单、系统托盘
- **Go Sidecar**: 处理所有 gRPC 逻辑、证书管理、proto 解析
- **Frontend**: React/Vue + TypeScript，UI 组件库待定

### Technology Stack
- **Backend (Sidecar)**: Go 1.25+
  - `google.golang.org/grpc` - gRPC 核心
  - `github.com/jhump/protoreflect` - Proto 解析与反射
- **Desktop Framework**: Tauri 2.x (Rust)
- **Frontend**: React 18 + TypeScript
  - UI 组件库: 待定 (shadcn/ui / Ant Design / Material-UI)
  - JSON 编辑器: Monaco Editor / CodeMirror
  - 状态管理: Zustand / Jotai
- **Communication**: Tauri Command / Event 机制与 Go Sidecar 通信
- **Storage**: 本地 SQLite 或 JSON 文件存储配置和历史

---

## MVP Scope & Phasing

### Phase 1: MVP (Required for Initial Launch)
**目标**: 功能完整的核心工具

- [x] Tauri + Go Sidecar 基础架构搭建
- [x] gRPC Reflection 服务发现
- [x] Proto 文件导入与解析
- [x] Unary 调用完整支持
- [x] Server Streaming 支持
- [x] Client Streaming 支持
- [x] Bidirectional Streaming 支持
- [x] Metadata 设置
- [x] TLS 基础配置（insecure/系统证书/自定义 CA）
- [x] 环境变量管理（基础版）
- [x] 请求收藏与恢复
- [x] 请求历史
- [x] 响应格式化与语法高亮

**MVP Definition**: 开发者可以连接任意 gRPC 服务，执行所有类型的调用，保存常用请求

### Phase 2: Enhancements (Post-Launch)
**目标**: 提升易用性和高级功能

- [ ] mTLS 双向认证
- [ ] 环境变量高级功能（全局变量、动态变量）
- [ ] 请求脚本/前置处理
- [ ] 响应断言与测试
- [ ] 导入/导出 Postman/Insomnia 集合
- [ ] 系统托盘快捷操作
- [ ] 快捷键与命令面板（Cmd+K）
- [ ] 主题切换（Dark/Light）

### Future Considerations
- [ ] gRPC-Web 支持
- [ ] 插件系统
- [ ] 团队协作/云端同步
- [ ] CLI 模式
- [ ] VS Code 插件

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation Strategy |
|------|------------|--------|---------------------|
| Tauri + Go Sidecar 通信复杂度 | 中 | 中 | 早期搭建原型验证通信机制，使用 stdin/stdout 或 HTTP localhost 通信 |
| Proto 文件解析复杂度高 | 中 | 高 | 使用成熟的 `protoreflect` 库，处理各种 proto 语法和 import |
| Streaming UI 性能问题 | 中 | 中 | 虚拟列表渲染大量消息，限制内存中消息数量，支持导出到文件 |
| 跨平台兼容性（Windows/Linux/macOS） | 低 | 高 | Tauri 天然跨平台，早期在各平台测试 |
| 证书管理安全性顾虑 | 低 | 高 | 证书本地存储，提供安全提示，敏感信息脱敏 |

---

## Dependencies & Blockers

**Dependencies:**
- Tauri 2.x 稳定版
- Go 1.25+ 开发环境
- 前端技术栈确定（React + UI 库）

**Known Blockers:**
- 无

---

## Appendix

### Glossary
- **Unary**: 单请求-单响应模式
- **Server Streaming**: 单请求-流式响应
- **Client Streaming**: 流式请求-单响应
- **Bidirectional Streaming**: 双向流式
- **Reflection**: gRPC 服务提供的自动发现协议定义的能力
- **Sidecar**: 与主应用一起运行的辅助进程

### References
- [ezy GitHub](https://github.com/getezy/ezy) - 参考产品
- [grpcui](https://github.com/fullstorydev/grpcui) - 参考功能
- [grpcurl](https://github.com/fullstorydev/grpcurl) - 命令行参考
- [Tauri Docs](https://tauri.app/)
- [Go gRPC Docs](https://pkg.go.dev/google.golang.org/grpc)

---

*This PRD was created through interactive requirements gathering with quality scoring to ensure comprehensive coverage of business, functional, UX, and technical dimensions.*
