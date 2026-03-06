# Next Codex 交接说明（2026-02-09）

你现在接手 `/Users/jacky/Codehub/grpcui` 的「项目-环境架构」落地工作。

## 总目标
基于 `docs/project-env-architecture.md` 完成项目化改造，并优化前端交互逻辑。当前已完成后端与 Rust 命令主链路，前端还有收尾与质量校验。

## 必须遵守
- 回复中文
- Go 项目收尾时执行：
  - `go mod tidy`
  - `golangci-lint run ./...`
  - `go test ./...`
- 按要求补充中文注释（尤其是新增 function）
- 尽量补充单元测试

## 已完成（不要回退）
### Go sidecar
- 已新增/改造项目域能力：
  - `Project` CRUD
  - `GetEnvironmentsByProject`
  - `GetCollectionsByProject`
  - `SetDefaultEnvironment`
  - `CloneProject`
- 新增文件：
  - `sidecar/internal/storage/project_store.go`
  - `sidecar/internal/storage/store_project_test.go`
  - `sidecar/api/routes_parse_test.go`
- 已改文件：
  - `sidecar/internal/storage/models.go`
  - `sidecar/internal/storage/store.go`
  - `sidecar/api/project_handlers.go`
  - `sidecar/api/routes.go`
  - `sidecar/internal/proto/parser.go`

### Rust Tauri
- 已扩展命令：项目 CRUD / clone / default env / 按项目查环境与集合
- 已改文件：
  - `src/src-tauri/src/commands.rs`
  - `src/src-tauri/src/main.rs`
- `cargo check` 当前通过（有 warning）

### 前端（进行中）
- API 适配层重写：`src/src/lib/tauriApi.ts`
- 请求面板、环境选择器、项目选择器、主界面已做大幅改造：
  - `src/src/components/RequestPanel.tsx`
  - `src/src/components/environment/EnvironmentSelector.tsx`
  - `src/src/components/project/ProjectSelector.tsx`（新增）
  - `src/src/App.tsx`
  - `src/src/hooks/useGrpcStream.ts`
  - `src/src/components/ServiceTree.tsx`（小改）

## 当前阻塞（优先修）
1. **TypeScript 类型文件损坏**
   - 文件：`src/src/types/index.ts`
   - 当前联合类型字面量被写坏（缺少引号），导致大量 TS2304。
   - 需要先修复此文件。

2. **前端类型检查未通过**
   - 命令：`cd src && npm run typecheck`
   - 先修 `types/index.ts` 后继续修其他潜在报错。

3. **Go lint 未通过（大量历史 + 新增问题）**
   - 命令：`cd sidecar && golangci-lint run ./...`
   - 当前报 32 条（errcheck/staticcheck），包含历史问题。
   - 要求：至少修复你本次改动相关的 lint 问题；如历史问题过多，给出清晰说明与分批计划。

## 已验证结果（供参考）
- `cd sidecar && go test ./... -count=1`：通过
- `cd sidecar && go mod tidy`：通过
- `cd src/src-tauri && cargo check`：通过（warning）
- `cd src && npm run typecheck`：失败（因 types 文件损坏）

## 你要继续做的事（按顺序）
1. 修复 `src/src/types/index.ts`（先恢复所有 string literal union）
2. 跑 `cd src && npm run typecheck`，逐个修复前端类型错误直到通过
3. 跑 `cd src && npm test`（至少单元测试通过；若失败要修）
4. 跑 `cd sidecar && go mod tidy`
5. 跑 `cd sidecar && golangci-lint run ./...`，修关键 lint
6. 跑 `cd sidecar && go test ./...`
7. 最终汇报：
   - 改了哪些文件
   - 哪些命令通过/未通过
   - 剩余风险

## 交付标准
- 前端 `npm run typecheck` 通过
- Go `go test ./...` 通过
- 关键交互可用：项目切换、项目环境选择、请求环境继承/指定、发送请求链路
- 结果说明清晰，包含绝对路径引用
