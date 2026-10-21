# CanonWeave 工程协作规则

本文件面向后续 Codex/AI 会话。发布仓库只提交用户运行、构建和使用 CanonWeave 所需的内容；内部开发交接、架构、数据模型、规格与进度文档保留在本地并被 Git 忽略。

## 文档读取

如果本地存在以下文件，开始改代码前按顺序阅读：

1. `docs/handoffs/2026-10-21-current-state.md`
2. `docs/handoffs/2026-10-21-next-agent-roadmap.md`
3. `docs/ARCHITECTURE.md`
4. `docs/DATA_MODEL.md`
5. `叙事织网_开发文档.md`
6. `开发进度.md`

这些文件属于本地开发资料，不得使用 `git add -f` 强制提交，除非作者明确要求。

如果本地没有这些文件，以 `README.md`、当前源码、迁移和测试为事实依据。

## 产品边界

- 产品是本地优先的 Tauri 2 桌面应用，不是 Web 服务。
- 项目权威数据在用户选择目录的 `narrative.sqlite`。
- React 只维护界面状态；领域写入必须通过 Tauri 命令。
- API Key 只能进入系统钥匙串，不得写入项目、日志或导出。
- 自动抽取、AI 对话、Agent 计划和生成结果先进入候选区，作者确认后才成为正式事实。
- Wiki、社区报告、索引、GraphRAG 摘要和 Embedding 检索结果都是派生视图，不能反向成为权威事实源。

## 当前高频入口

- Agent / 协作助手：`src/features/agent/AgentPage.tsx`
- 正文：`src/features/studio/StudioPage.tsx`
- 种子卡与叙事蓝本：`src/features/seed/SeedPage.tsx`
- 卷章编排：`src/features/seed/OutlinePanel.tsx`
- 图形化因果链：`src/features/seed/LogicGraph.tsx`
- 知识域：`src/features/knowledge/KnowledgePage.tsx`
- 人物：`src/features/characters/CharactersPage.tsx`
- 审计：`src/features/audit/AuditPage.tsx`
- Rust 核心：`src-tauri/src/`
- DB 迁移：`src-tauri/migrations/`
- 前端 IPC：`src/lib/ipc.ts`、`src/lib/bindings.ts`

## 下一阶段优先级

### P0：Agent 化

- 增加持久化 `agent_run`、`agent_step` 和证据记录。
- 建立工具注册表与权限策略：只读工具自动执行，写入工具只生成候选。
- 支持计划、执行、观察、重试、暂停、取消和恢复。
- Agent 能扫描现有正文与资料，提出人物、关系、势力、能力、时间线、伏笔和知识条目候选。
- UI 改为 Agent 优先的项目工作台，保留资料库页面作为权威数据入口。

### 规划门槛

- 在卷章规划前建立“最低可运行叙事地基”：核心人物与对手、关系张力、势力、关键能力及代价/边界、时间线锚点、关键地点和物品、核心戏剧问题。
- 不需要一次补完全部设定。
- 初版骨架后，允许 Agent 在同一个候选变更组中同时提出骨架、人物、关系、时间线、能力和知识库变更。

### P1：可选 Embedding

- Embedding 不是运行前提；关闭时继续使用 FTS5 + PPR。
- 开启后使用 FTS + 向量 + PPR 混合检索。
- 源内容哈希变化后向量必须失效并可重建。
- 模型不可用时自动降级。

## 不可违反的领域规则

- 候选未确认时不得进入正文或权威事实表。
- 正式事实必须保留来源片段或明确的作者确认来源。
- 不得让摘要替代原文证据。
- Agent 不得直接调用“确认候选”“提交正文”“覆盖权威事实”等写入能力。
- 删除、批量应用和永久例外必须由作者确认。
- 不要把场景重新变成正文强制入口。
- 不要把章节数固定为十章；作者决定篇幅结构。
- 新增知识域时保持：知识条目 → 图谱实体 → FTS → 上下文 → Wiki 的链路。
- 任何 schema 修改都必须新增迁移，并更新 `SCHEMA_VERSION`、旧项目升级和重开测试。

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

Playwright 在受限沙箱中可能无法启动 Chromium；需要时使用非沙箱执行。当前没有 `src/**/*.test.ts(x)` 单元测试，页面验证以 Playwright 为主。

## 开发注意事项

- 不要运行全仓 `cargo fmt`，只格式化本次实际修改的 Rust 文件。
- 修改 `#[tauri::command]` 或领域类型后，运行 `cargo test bindings_tests::export_typescript_bindings -- --exact` 更新 `src/lib/bindings.ts`。
- 不要手改 `src/lib/bindings.ts` 的生成区；手写封装放在 `src/lib/ipc.ts`。
- React 不得直接访问 SQLite。
- 写操作必须通过 Tauri 命令和事务边界。
- 长模型请求优先使用流式输出，并提供瞬时错误重试与可读错误信息。
- `docs/images/` 是 README 使用的用户界面截图，仍属于发布内容。

## 提交前

1. 检查 `git status` 与 `git diff --check`。
2. 运行 Rust 测试、前端构建和 Playwright。
3. 确认没有内部交接文档、`.codex`/`.agents`、API Key、测试数据库或本机绝对路径进入提交。
4. 汇报实际验证结果和仍未验证的部分。
