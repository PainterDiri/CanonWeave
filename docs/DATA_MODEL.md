# CanonWeave 数据模型

## 数据库边界

- SQLite 文件：`<项目目录>/narrative.sqlite`
- 当前 schema：v6
- 迁移目录：`src-tauri/migrations/`
- 迁移入口：`src-tauri/src/db/mod.rs`
- 所有项目数据、索引、版本和派生视图都在项目目录内。
- API Key 不属于项目数据，只保存在系统钥匙串。

## 数据分层

| 层 | 代表表 | 性质 |
| --- | --- | --- |
| 项目与结构 | `project`、`premise`、`outline_node`、`story_structure`、`chapter`、`scene` | 作者确认的项目结构和正文 |
| 权威故事事实 | `entity`、`claim`、`event`、`relation`、`promise`、`arc_beat` | 带来源和状态的叙事事实 |
| 人物与知识 | `character_core`、`knowledge_domain`、`knowledge_entry` | 人物核心、环境、物品和自定义知识 |
| 候选与审核 | `candidate`、`issue`、`rule`、`review_policy`、`review_batch` | 未确认内容、冲突和审核策略 |
| 表达控制 | `style_profile`、`style_sample`、`character_voice_sample`、`character_voice_profile`、`motif`、`preference` | 文笔、人物声音和作者偏好 |
| 历史与运行 | `snapshot`、`revision`、`generation_run`、`audit_log` | 版本、生成和审计记录 |
| 派生视图 | `text_fts`、`graph_community`、`wiki_page`、`insight` | 可重建，不是权威事实 |

## 关键不变量

### 候选隔离

- `candidate.status='pending'` 时不得进入正文事实表。
- 自动抽取、对话采纳和生成草稿都必须先进入候选。
- 删除会话不删除候选或已确认事实。

### 事实来源

- 正式事实必须有 `source_span` 或明确的作者确认来源。
- `claim.truth_layer` 区分 `world`、`belief`、`utterance`、`narrative`、`planned`。
- 不同真值层不能直接按同一世界事实判冲突。

### 卷章结构

- 分卷模式下，卷是 `outline_node(level='volume')`。
- 章节通过 `outline_node.parent_id` 指向卷。
- 章节确认后，`chapter.volume_id` 指向卷节点。
- 章节号在全书连续；卷内顺序由父卷和章节号共同决定。
- 单卷模式和分卷模式都使用同一章节主键与版本机制。

### 知识域

- `knowledge_domain` 定义作者可见名称和动态字段 schema。
- 内置 `environment`、`item` 由项目打开时的 `ensure_defaults` 保证存在。
- `knowledge_entry.entity_id` 指向自动同步的图谱实体。
- 知识条目的摘要和字段会写入 `text_fts(source_kind='knowledge')`。
- 知识条目变更后，社区报告和 Wiki 标记过期。

### 版本与派生

- 正文 revision 不回删历史；回滚会生成新版本。
- Wiki 和社区摘要必须可全量重建。
- `text_fts`、GraphRAG 摘要和 Wiki 不得作为事实写回源。

## schema 迁移

| 版本 | 内容 |
| --- | --- |
| v1 | 项目、种子、会话、候选、骨架、实体、事实、事件、关系、伏笔、规则、版本、Wiki、FTS |
| v2 | 生成运行与候选关联 |
| v3 | 审核批次、抽样、例外、冲突指纹、精修文本 |
| v4 | 分支感知事实、社区标题、全书洞察 |
| v5 | Wiki 来源片段、人物声音样本与质心、伏笔证据拆分 |
| v6 | 章节卷归属、作品结构、知识域、知识条目 |

旧项目打开时：

1. 按 `schema_migration` 顺序执行缺失迁移。
2. 补齐默认知识域。
3. 更新数据库和 `project.json` 的 schema 版本。
4. 不主动改写已有事实或正文。
