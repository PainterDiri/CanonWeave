ALTER TABLE project ADD COLUMN current_branch_id TEXT;
UPDATE project SET current_branch_id = 'main-' || id;
ALTER TABLE candidate ADD COLUMN branch_id TEXT;
ALTER TABLE graph_community ADD COLUMN title TEXT NOT NULL DEFAULT '';

CREATE TABLE insight (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  question TEXT NOT NULL,
  answer TEXT NOT NULL,
  source_ids TEXT NOT NULL DEFAULT '[]',
  community_ids TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL
);
CREATE INDEX idx_insight_project ON insight(project_id, created_at);
