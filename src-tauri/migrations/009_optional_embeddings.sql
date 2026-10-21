ALTER TABLE embedding ADD COLUMN content TEXT NOT NULL DEFAULT '';
ALTER TABLE embedding ADD COLUMN content_hash TEXT NOT NULL DEFAULT '';
ALTER TABLE embedding ADD COLUMN status TEXT NOT NULL DEFAULT 'stale';
ALTER TABLE embedding ADD COLUMN branch_id TEXT;
ALTER TABLE embedding ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';

DELETE FROM embedding
WHERE rowid NOT IN (
  SELECT MAX(rowid)
  FROM embedding
  GROUP BY project_id, owner_kind, owner_id, model
);

CREATE UNIQUE INDEX idx_embedding_owner_model
  ON embedding(project_id, owner_kind, owner_id, model);
CREATE INDEX idx_embedding_status
  ON embedding(project_id, model, dim, status);

UPDATE embedding
SET content_hash = id,
    updated_at = created_at
WHERE content_hash = '';

UPDATE project SET schema_version = 9;
