ALTER TABLE review_batch ADD COLUMN snapshot_id TEXT;
ALTER TABLE review_batch ADD COLUMN policy_mode TEXT NOT NULL DEFAULT 'balanced';
ALTER TABLE review_batch ADD COLUMN sample_rate INTEGER NOT NULL DEFAULT 0;
ALTER TABLE review_batch ADD COLUMN sampled_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE review_batch ADD COLUMN undone_at TEXT;

CREATE TABLE review_sample (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  batch_id TEXT NOT NULL,
  candidate_id TEXT NOT NULL,
  sampled INTEGER NOT NULL DEFAULT 0,
  review_status TEXT NOT NULL DEFAULT 'pending',
  reviewed_at TEXT,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_review_sample_project ON review_sample(project_id, sampled, review_status);

CREATE TABLE rule_exception (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  rule_id TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  reason TEXT NOT NULL,
  permanent INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_rule_exception_project ON rule_exception(project_id, rule_id, fingerprint);

ALTER TABLE issue ADD COLUMN fingerprint TEXT NOT NULL DEFAULT '';
CREATE INDEX idx_issue_fingerprint ON issue(project_id, fingerprint, status);

ALTER TABLE generation_run ADD COLUMN raw_text TEXT NOT NULL DEFAULT '';
ALTER TABLE generation_run ADD COLUMN refined_text TEXT NOT NULL DEFAULT '';
