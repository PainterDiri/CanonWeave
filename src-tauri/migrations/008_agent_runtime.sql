CREATE TABLE agent_run (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  session_id TEXT REFERENCES chat_session(id) ON DELETE SET NULL,
  mode TEXT NOT NULL DEFAULT 'propose',
  scope TEXT NOT NULL DEFAULT 'project',
  output TEXT NOT NULL DEFAULT 'all',
  goal TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft',
  plan_json TEXT NOT NULL DEFAULT '[]',
  current_step INTEGER NOT NULL DEFAULT 0,
  approval_policy TEXT NOT NULL DEFAULT 'manual',
  model TEXT NOT NULL DEFAULT '',
  context_hash TEXT NOT NULL DEFAULT '',
  error TEXT NOT NULL DEFAULT '',
  started_at TEXT,
  finished_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_agent_run_project
  ON agent_run(project_id, status, updated_at);

CREATE TABLE agent_step (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES agent_run(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,
  kind TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  input_json TEXT NOT NULL DEFAULT '{}',
  output_json TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL DEFAULT 'pending',
  risk_level TEXT NOT NULL DEFAULT 'read',
  requires_approval INTEGER NOT NULL DEFAULT 0,
  candidate_ids TEXT NOT NULL DEFAULT '[]',
  evidence_json TEXT NOT NULL DEFAULT '[]',
  error TEXT NOT NULL DEFAULT '',
  started_at TEXT,
  finished_at TEXT,
  UNIQUE(run_id, seq)
);
CREATE INDEX idx_agent_step_run ON agent_step(run_id, seq);

CREATE TABLE agent_evidence (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES agent_run(id) ON DELETE CASCADE,
  step_id TEXT NOT NULL REFERENCES agent_step(id) ON DELETE CASCADE,
  source_kind TEXT NOT NULL,
  source_id TEXT NOT NULL,
  source_span TEXT NOT NULL,
  quote TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_agent_evidence_run ON agent_evidence(run_id, step_id);

ALTER TABLE candidate ADD COLUMN agent_run_id TEXT REFERENCES agent_run(id) ON DELETE SET NULL;
ALTER TABLE candidate ADD COLUMN agent_step_id TEXT REFERENCES agent_step(id) ON DELETE SET NULL;
CREATE INDEX idx_candidate_agent_run ON candidate(agent_run_id, status);

UPDATE project SET schema_version = 8;
