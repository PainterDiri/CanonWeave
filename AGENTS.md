# CanonWeave 开发交接规则

本文件面向后续 Codex/AI 会话。开始改代码前，按顺序阅读：

1. [当前工程交接](docs/handoffs/2026-10-21-current-state.md)
2. [当前架构](docs/ARCHITECTURE.md)
3. [数据模型](docs/DATA_MODEL.md)
4. [开发规格](叙事织网_开发文档.md)
5. [开发进度](开发进度.md)

## 项目边界

- 产品是本地优先的 Tauri 2 桌面应用，不是 Web 服务。
- 项目权威数据在用户选择目录的 `narrative.sqlite`。
- React 只维护界面状态；领域写入必须通过 Tauri 命令。
- API Key 只能进入系统钥匙串，不得写入项目、日志或导出。
- 自动抽取、AI 对话和生成结果先进入候选区，作者确认后才成为正式事实。
- Wiki、社区报告、索引和 GraphRAG 摘要都是派生视图，不能反向成为权威事实源。

## 当前高频入口

- 正文：`src/features/studio/StudioPage.tsx`
- 卷章规划：`src/features/seed/OutlinePanel.tsx`
- 协作助手：`src/features/agent/AgentPage.tsx`
- 知识域：`src/features/knowledge/KnowledgePage.tsx`
- 人物：`src/features/characters/CharactersPage.tsx`
- 审计：`src/features/audit/AuditPage.tsx`
- Rust 核心：`src-tauri/src/`
- DB 迁移：`src-tauri/migrations/`
- 前端 IPC：`src/lib/ipc.ts`、`src/lib/bindings.ts`

## 验证命令

在 `src-tauri/`：

```bash
cargo test
cargo check
```

在仓库根目录：

```bash
npm run build
npx playwright test
```

当前 Vitest 命令已配置，但仓库还没有 `src/**/*.test.ts(x)` 测试文件。页面验证以 Playwright 为主。

## 开发注意事项

- 不要运行全仓 `cargo fmt`。该仓库既有代码并未统一格式化，全仓格式化会产生大量无关差异。只格式化本次新建或实际修改的 Rust 文件。
- 修改 `#[tauri::command]` 或领域类型后，运行 `cargo test bindings_tests::export_typescript_bindings -- --exact` 更新 `src/lib/bindings.ts`。
- 不要手改 `src/lib/bindings.ts` 中的生成区；需要手写封装时改 `src/lib/ipc.ts`。
- 不要直接写 SQLite 的 React 代码；通过命令边界提交事务。
- 不要把场景重新变成正文强制入口。场景是可选分场工具和底层生成单元。
- 不要把章节数重新固定为十章。作者决定单卷章节数或每卷章节数。
- 新增知识域时必须保持：知识条目 → 图谱实体 → FTS → 上下文 → Wiki 的链路。
- 任何涉及 schema 的修改都要新增迁移，并更新 `SCHEMA_VERSION`、旧项目升级和重开测试。

## 提交前

1. 检查 `git diff --check`。
2. 运行 Rust 测试、前端构建和 Playwright。
3. 确认没有 API Key、测试项目数据库或本机绝对路径进入提交。
4. 汇报实际验证结果和仍未验证的部分。
