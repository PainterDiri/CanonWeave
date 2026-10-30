-- v11：Agent harness 轨迹、作者守则、章节工艺卡与叙事线程。

-- 对话消息可回到产生它的 Agent Run（工具调用轨迹、候选与证据）。
ALTER TABLE chat_message ADD COLUMN agent_run_id TEXT REFERENCES agent_run(id) ON DELETE SET NULL;

-- 作者守则：项目级长期指令（文风底线、禁写内容、命名约定等），
-- 注入 Agent 与正文生成的任务契约层；它不是世界事实。
CREATE TABLE project_guide (
  project_id TEXT PRIMARY KEY REFERENCES project(id) ON DELETE CASCADE,
  content TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL
);

-- 章节工艺卡：价值转变、危机抉择、章末钩子与读者认知揭示。
ALTER TABLE outline_node ADD COLUMN value_shift TEXT NOT NULL DEFAULT '';
ALTER TABLE outline_node ADD COLUMN crisis TEXT NOT NULL DEFAULT '';
ALTER TABLE outline_node ADD COLUMN hook TEXT NOT NULL DEFAULT '';
ALTER TABLE outline_node ADD COLUMN reveal TEXT NOT NULL DEFAULT '';

-- 叙事线程（环境/疑问/人物/事件）：记录开启与关闭章节，检查先开后关的嵌套顺序。
CREATE TABLE story_thread (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('milieu','inquiry','character','event')),
  title TEXT NOT NULL,
  question TEXT NOT NULL DEFAULT '',
  open_chapter INTEGER,
  close_chapter INTEGER,
  status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','closed','dropped')),
  notes TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_story_thread_project ON story_thread(project_id, open_chapter);

UPDATE project SET schema_version = 11;
