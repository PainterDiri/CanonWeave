PRAGMA foreign_keys = ON;

CREATE TABLE project (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  root_path TEXT NOT NULL UNIQUE,
  schema_version INTEGER NOT NULL DEFAULT 1,
  writing_language TEXT NOT NULL DEFAULT 'zh-CN',
  mode TEXT NOT NULL DEFAULT 'idea',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE premise (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  logline TEXT NOT NULL,
  genre TEXT NOT NULL DEFAULT '',
  subgenre TEXT NOT NULL DEFAULT '',
  theme TEXT NOT NULL DEFAULT '',
  emotional_promise TEXT NOT NULL DEFAULT '',
  tone TEXT NOT NULL DEFAULT '',
  core_conflict TEXT NOT NULL DEFAULT '',
  protagonist_desire TEXT NOT NULL DEFAULT '',
  flaw TEXT NOT NULL DEFAULT '',
  secret TEXT NOT NULL DEFAULT '',
  world_hook TEXT NOT NULL DEFAULT '[]',
  pov TEXT NOT NULL DEFAULT '',
  person TEXT NOT NULL DEFAULT '',
  tense TEXT NOT NULL DEFAULT '',
  target_length TEXT NOT NULL DEFAULT '',
  update_rhythm TEXT NOT NULL DEFAULT '',
  ending_intent TEXT NOT NULL DEFAULT '',
  taboo TEXT NOT NULL DEFAULT '[]',
  status TEXT NOT NULL DEFAULT 'candidate',
  source_message_id TEXT,
  field_sources TEXT NOT NULL DEFAULT '{}',
  version INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_premise_project ON premise(project_id, status);

CREATE TABLE chat_session (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  scope TEXT NOT NULL DEFAULT 'seed',
  scene_id TEXT,
  branch_id TEXT,
  parent_session_id TEXT REFERENCES chat_session(id) ON DELETE SET NULL,
  mode TEXT NOT NULL DEFAULT 'diverge',
  title TEXT NOT NULL DEFAULT '新会话',
  model TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'active',
  summary TEXT NOT NULL DEFAULT '',
  summary_hash TEXT NOT NULL DEFAULT '',
  last_message_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  pinned INTEGER NOT NULL DEFAULT 0,
  archived_at TEXT
);
CREATE INDEX idx_chat_session_project ON chat_session(project_id, archived_at, last_message_at);

CREATE TABLE chat_message (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES chat_session(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  model TEXT NOT NULL DEFAULT '',
  tokens INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  linked_candidate_ids TEXT NOT NULL DEFAULT '[]',
  pinned INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_chat_message_session ON chat_message(session_id, seq);

CREATE TABLE candidate (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  payload TEXT NOT NULL,
  source_span TEXT,
  source_message_id TEXT,
  confidence REAL NOT NULL DEFAULT 0.5,
  risk_level TEXT NOT NULL DEFAULT 'L2',
  status TEXT NOT NULL DEFAULT 'pending',
  batch_id TEXT,
  created_at TEXT NOT NULL,
  confirmed_at TEXT
);
CREATE INDEX idx_candidate_project ON candidate(project_id, status, risk_level);
CREATE INDEX idx_candidate_batch ON candidate(batch_id);

CREATE TABLE outline_node (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  parent_id TEXT REFERENCES outline_node(id) ON DELETE CASCADE,
  level TEXT NOT NULL DEFAULT 'chapter',
  order_no INTEGER NOT NULL,
  title TEXT NOT NULL,
  goal TEXT NOT NULL DEFAULT '',
  conflict TEXT NOT NULL DEFAULT '',
  turn TEXT NOT NULL DEFAULT '',
  payoff TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'candidate',
  branch_id TEXT,
  source TEXT NOT NULL DEFAULT 'author',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_outline_project ON outline_node(project_id, order_no);

CREATE TABLE source_document (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  path TEXT NOT NULL,
  hash TEXT NOT NULL,
  type TEXT NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  word_count INTEGER NOT NULL DEFAULT 0,
  imported_at TEXT NOT NULL
);
CREATE INDEX idx_source_document_project ON source_document(project_id);

CREATE TABLE branch (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  parent_branch_id TEXT,
  forked_at_revision TEXT,
  status TEXT NOT NULL DEFAULT 'active',
  created_at TEXT NOT NULL
);

CREATE TABLE chapter (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  number INTEGER NOT NULL,
  title TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'planned',
  current_revision_id TEXT,
  style_profile_id TEXT,
  style_epoch INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_chapter_project ON chapter(project_id, number);

CREATE TABLE scene (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  chapter_id TEXT NOT NULL REFERENCES chapter(id) ON DELETE CASCADE,
  number INTEGER NOT NULL,
  goal TEXT NOT NULL DEFAULT '',
  conflict TEXT NOT NULL DEFAULT '',
  pov TEXT NOT NULL DEFAULT '',
  story_time TEXT NOT NULL DEFAULT '',
  branch_id TEXT,
  place_id TEXT,
  style_profile_id TEXT,
  emotion_beat TEXT NOT NULL DEFAULT '{}',
  sensory_plan TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL DEFAULT 'planned',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_scene_chapter ON scene(chapter_id, number);

CREATE TABLE entity (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  type TEXT NOT NULL,
  canonical_name TEXT NOT NULL,
  aliases TEXT NOT NULL DEFAULT '[]',
  status TEXT NOT NULL DEFAULT 'confirmed',
  description TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_entity_project_type ON entity(project_id, type);

CREATE TABLE character_core (
  entity_id TEXT PRIMARY KEY REFERENCES entity(id) ON DELETE CASCADE,
  desire TEXT NOT NULL DEFAULT '',
  need TEXT NOT NULL DEFAULT '',
  fear TEXT NOT NULL DEFAULT '',
  flaw TEXT NOT NULL DEFAULT '',
  "values" TEXT NOT NULL DEFAULT '[]',
  secret TEXT NOT NULL DEFAULT '',
  taboo TEXT NOT NULL DEFAULT '[]',
  voice_anchor TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL
);

CREATE TABLE claim (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  subject TEXT NOT NULL,
  subject_id TEXT,
  predicate TEXT NOT NULL,
  object TEXT NOT NULL DEFAULT '',
  object_entity_id TEXT,
  truth_layer TEXT NOT NULL DEFAULT 'world',
  perspective_id TEXT,
  valid_from TEXT,
  valid_to TEXT,
  branch_id TEXT,
  source_span TEXT NOT NULL DEFAULT '',
  confidence REAL NOT NULL DEFAULT 0.5,
  status TEXT NOT NULL DEFAULT 'candidate',
  locked INTEGER NOT NULL DEFAULT 0,
  version INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_claim_project ON claim(project_id, status, truth_layer);
CREATE INDEX idx_claim_subject ON claim(project_id, subject_id, predicate);

CREATE TABLE event (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  participants TEXT NOT NULL DEFAULT '[]',
  place_id TEXT,
  story_time TEXT NOT NULL DEFAULT '',
  reveal_time TEXT NOT NULL DEFAULT '',
  cause TEXT NOT NULL DEFAULT '',
  outcome TEXT NOT NULL DEFAULT '',
  branch_id TEXT,
  source_span TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'candidate',
  created_at TEXT NOT NULL
);

CREATE TABLE relation (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  from_id TEXT NOT NULL,
  to_id TEXT NOT NULL,
  type TEXT NOT NULL,
  direction TEXT NOT NULL DEFAULT 'directed',
  valid_from TEXT,
  valid_to TEXT,
  evidence_id TEXT,
  status TEXT NOT NULL DEFAULT 'candidate',
  created_at TEXT NOT NULL
);
CREATE INDEX idx_relation_from ON relation(project_id, from_id, type);

CREATE TABLE promise (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'planned',
  plant_scene_id TEXT,
  target_window TEXT NOT NULL DEFAULT '',
  payoff_contract TEXT NOT NULL DEFAULT '',
  evidence_ids TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_promise_project ON promise(project_id, state);

CREATE TABLE arc_beat (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  character_id TEXT NOT NULL,
  chapter_id TEXT,
  before_state TEXT NOT NULL DEFAULT '',
  event TEXT NOT NULL DEFAULT '',
  after_state TEXT NOT NULL DEFAULT '',
  evidence_id TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE style_profile (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  scope TEXT NOT NULL DEFAULT 'project',
  scope_id TEXT,
  name TEXT NOT NULL,
  pov TEXT NOT NULL DEFAULT 'third_limited',
  person TEXT NOT NULL DEFAULT 'third',
  tense TEXT NOT NULL DEFAULT 'past',
  syntax TEXT NOT NULL DEFAULT '{}',
  lexicon TEXT NOT NULL DEFAULT '{}',
  imagery TEXT NOT NULL DEFAULT '{}',
  rhetoric TEXT NOT NULL DEFAULT '{}',
  emotion TEXT NOT NULL DEFAULT '{}',
  ratios TEXT NOT NULL DEFAULT '{}',
  anti_style TEXT NOT NULL DEFAULT '[]',
  version INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL DEFAULT 'candidate',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_style_profile_project ON style_profile(project_id, scope, scope_id);

CREATE TABLE style_sample (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  profile_id TEXT REFERENCES style_profile(id) ON DELETE CASCADE,
  text TEXT NOT NULL,
  source_span TEXT NOT NULL DEFAULT '',
  source_type TEXT NOT NULL DEFAULT 'author',
  approved INTEGER NOT NULL DEFAULT 1,
  tags TEXT NOT NULL DEFAULT '[]',
  embedding_id TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE motif (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  meaning TEXT NOT NULL DEFAULT '',
  channels TEXT NOT NULL DEFAULT '[]',
  first_scene_id TEXT,
  last_scene_id TEXT,
  use_count INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'active',
  created_at TEXT NOT NULL
);

CREATE TABLE preference (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  scene_id TEXT,
  left_run_id TEXT,
  right_run_id TEXT,
  chosen_run_id TEXT,
  reason TEXT NOT NULL DEFAULT '',
  dimension_scores TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);

CREATE TABLE rule (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  code TEXT NOT NULL,
  constraint_layer TEXT NOT NULL DEFAULT 'hard',
  severity TEXT NOT NULL DEFAULT 'block',
  predicate TEXT NOT NULL DEFAULT '{}',
  exception_scope TEXT NOT NULL DEFAULT '{}',
  description TEXT NOT NULL DEFAULT '',
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_rule_project_code ON rule(project_id, code);

CREATE TABLE issue (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  rule_id TEXT NOT NULL,
  severity TEXT NOT NULL DEFAULT 'warn',
  claim_ids TEXT NOT NULL DEFAULT '[]',
  evidence_spans TEXT NOT NULL DEFAULT '[]',
  explanation TEXT NOT NULL DEFAULT '',
  certificate TEXT NOT NULL DEFAULT '{}',
  solver_core TEXT NOT NULL DEFAULT '[]',
  metric_snapshot TEXT NOT NULL DEFAULT '{}',
  risk_level TEXT NOT NULL DEFAULT 'L2',
  batch_id TEXT,
  deferred_until TEXT,
  status TEXT NOT NULL DEFAULT 'open',
  created_at TEXT NOT NULL
);
CREATE INDEX idx_issue_project ON issue(project_id, status, severity);

CREATE TABLE review_policy (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  scope TEXT NOT NULL DEFAULT 'project',
  scope_id TEXT,
  mode TEXT NOT NULL DEFAULT 'balanced',
  auto_accept_level TEXT NOT NULL DEFAULT 'L0',
  sample_rate REAL NOT NULL DEFAULT 0.0,
  lock_hard INTEGER NOT NULL DEFAULT 1,
  updated_at TEXT NOT NULL
);

CREATE TABLE review_batch (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  chapter_id TEXT,
  policy_id TEXT,
  item_count INTEGER NOT NULL DEFAULT 0,
  accepted_count INTEGER NOT NULL DEFAULT 0,
  rejected_count INTEGER NOT NULL DEFAULT 0,
  revision_id TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE snapshot (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  chapter_id TEXT,
  content TEXT NOT NULL DEFAULT '',
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);

CREATE TABLE revision (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  parent_id TEXT,
  chapter_id TEXT,
  diff TEXT NOT NULL DEFAULT '{}',
  snapshot_id TEXT,
  author_action TEXT NOT NULL DEFAULT 'manual',
  created_at TEXT NOT NULL
);

CREATE TABLE generation_run (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  chapter_id TEXT,
  scene_id TEXT,
  model TEXT NOT NULL DEFAULT '',
  params TEXT NOT NULL DEFAULT '{}',
  context_hash TEXT NOT NULL DEFAULT '',
  prompt_version TEXT NOT NULL DEFAULT '',
  style_profile_version INTEGER NOT NULL DEFAULT 0,
  generation_mode TEXT NOT NULL DEFAULT 'quick',
  review_mode TEXT NOT NULL DEFAULT 'balanced',
  fact_delta TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL DEFAULT 'running',
  error TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL
);

CREATE TABLE graph_community (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  level INTEGER NOT NULL DEFAULT 0,
  member_ids TEXT NOT NULL DEFAULT '[]',
  summary_hash TEXT NOT NULL DEFAULT '',
  content TEXT NOT NULL DEFAULT '',
  source_ids TEXT NOT NULL DEFAULT '[]',
  generated_at TEXT NOT NULL,
  stale_at TEXT
);

CREATE TABLE wiki_page (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  entity_id TEXT,
  kind TEXT NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  content TEXT NOT NULL DEFAULT '',
  content_hash TEXT NOT NULL DEFAULT '',
  source_ids TEXT NOT NULL DEFAULT '[]',
  generated_at TEXT NOT NULL,
  stale_at TEXT
);
CREATE INDEX idx_wiki_project ON wiki_page(project_id, kind);

CREATE TABLE audit_log (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  action TEXT NOT NULL,
  subject TEXT NOT NULL DEFAULT '',
  policy_id TEXT,
  batch_id TEXT,
  detail TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);

CREATE TABLE embedding (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  owner_kind TEXT NOT NULL,
  owner_id TEXT NOT NULL,
  model TEXT NOT NULL,
  dim INTEGER NOT NULL,
  vector BLOB NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_embedding_owner ON embedding(project_id, owner_kind, owner_id);

CREATE VIRTUAL TABLE text_fts USING fts5(
  content,
  tokenized,
  source_id UNINDEXED,
  project_id UNINDEXED,
  source_kind UNINDEXED,
  chapter_id UNINDEXED,
  branch_id UNINDEXED,
  tokenize = 'unicode61'
);
