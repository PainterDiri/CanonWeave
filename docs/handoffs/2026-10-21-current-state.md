# 2026-10-21 CanonWeave 工程交接

## 基线

- 日期：2026-10-21
- 分支：`main`
- 代码基线提交：`30867c3 feat: add scalable story structures and knowledge domains`
- 远端：`https://github.com/PainterDiri/CanonWeave.git`
- 该提交推送时，工作树干净，`main` 与 `origin/main` 一致。

本文档在该代码基线之后增加，用于后续会话接续。若工作树更新，先执行：

```bash
git status --short --branch
git log -5 --oneline
```

## 当前产品状态

CanonWeave 已从固定十章原型升级为可伸缩的长篇工作台：

- 作者定义卷章：1 至 1200 章，或最多 40 卷并逐卷定义章节数。
- 章节骨架分批生成，卷、章归属一致，正文按卷分组。
- 正文默认按章写作，场景降为可选分场工具。
- 知识库与人物同级：内置环境、重要物品，支持禁墟、神道、灾厄等自定义知识域。
- 知识条目接入实体图谱、FTS5、上下文、GraphRAG 和 Narrative Wiki。
- 协作助手独立成页，可运行巡检、图谱和 Wiki 工具，并读取项目只读快照。
- 多视角真值、Z3 硬约束、冲突证书、人物声音、文笔漂移、版本和导出继续可用。

当前 schema：v6。

## 当前导航

```text
概览
写作：正文 / 协作助手
故事资料：人物 / 知识库 / 时间线 / 伏笔 / 叙事百科
质量：一致性审计
项目：灵感与卷章 / 设定与配置
```

这是有意的优先顺序。后续不要无故把“正文”和“协作助手”重新放到低频分组。

## 关键实现

### 卷章

- `src-tauri/src/commands/outline.rs`
- `src-tauri/src/outline.rs`
- `src/features/seed/OutlinePanel.tsx`
- 章节骨架按每批 16 章生成。
- 一批失败不提交半成品。
- 已有已确认骨架时，不允许静默用新计划覆盖。

### 知识域

- `src-tauri/src/knowledge.rs`
- `src-tauri/src/commands/knowledge.rs`
- `src/features/knowledge/KnowledgePage.tsx`
- 内置域：`environment`、`item`。
- 自定义域字段保存在 `knowledge_domain.field_schema`。
- 条目字段保存在 `knowledge_entry.fields`。
- 条目保存时同步 `entity` 和 `text_fts`。

### 协作助手

- `src/features/agent/AgentPage.tsx`
- `src/features/seed/ChatWorkspace.tsx`
- `src-tauri/src/commands/chat.rs`
- 目前是“受限任务编排”，不是无边界自主代理。
- 直接工具：一致性巡检、社区摘要重建、Wiki 重建。
- 写入仍必须通过候选和作者确认。

### 正文

- `src/features/studio/StudioPage.tsx`
- 默认不展示分场列表。
- 打开“分场编辑”后才显示场景。
- 底层仍使用 `scene` 作为上下文和生成事务单元。

## 已验证

- `cargo test`：57 项通过。
- `npm run build`：通过。
- `npx playwright test`：2 项通过。
- 页面覆盖：新导航顺序、卷章页、独立协作助手、知识库、人物、审计、正文、图谱、Wiki 和导出。
- 前端分块：初始主包约 455.57KB，正文、卷章、助手、知识库和 Wiki 独立 chunk。

未做真实模型联网验收；模型调用测试使用 mock 或本地单元逻辑。

## 已知限制

1. 超长卷章结构会发起多次模型请求，目前没有逐批进度事件和部分断点续跑。
2. 知识域支持动态文本字段，但尚无跨知识条目的专用关系边编辑器。
3. 协作助手能调用固定工具，但还没有持久化“多步计划、执行轨迹、每步审批”的完整 Agent Run。
4. GraphRAG 社区检测仍是一级加权标签传播，尚未替换为 Leiden 或分层社区。
5. 分支骨架的完整分叉、比较和合并仍未实现。
6. 真实供应商模型对长 JSON、流式取消和结构化输出的兼容性仍需密钥验收。
7. Vitest 已安装和配置，但当前没有 `src/**/*.test.ts(x)` 页面单元测试。

## 推荐的下一步

### P0：长结构可靠性

- 给卷章生成增加事件流：当前卷、当前批次、已生成章节数、预计剩余批次。
- 支持暂停、取消和从已成功批次继续。
- 将生成结果分批写为候选，但保持“作者确认前不成为正文结构”的边界。

### P1：知识域表达能力

- 字段类型增加枚举、日期、实体引用、数值区间和列表。
- 增加知识条目之间的关系编辑器。
- 为知识域增加结构校验和重命名/字段迁移预览。

### P1：协作助手

- 增加持久化 `agent_run` 与步骤记录。
- 把“巡检 → 定位证据 → 生成修复方案 → 等待确认”做成可恢复任务。
- 工具执行继续遵守只读自动、写入审批的原则。

### P2：检索与叙事质量

- 接入真实嵌入模型并测试向量降级。
- 分层社区与图检索评测。
- 人物声音语义距离与盲评基准。

## 新会话启动清单

1. 读 [架构](../ARCHITECTURE.md) 和 [数据模型](../DATA_MODEL.md)。
2. 读 [开发规格](../../叙事织网_开发文档.md) 中与当前任务相关的章节。
3. 检查 `git status`，不要覆盖未提交改动。
4. 修改数据库时新增迁移，不直接改旧迁移。
5. 修改前端后检查桌面宽度 1280×720 和 1440×900。
6. 按 [AGENTS.md](../../AGENTS.md) 中的命令验证。
7. 不要运行全仓 `cargo fmt`。
