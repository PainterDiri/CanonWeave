ALTER TABLE chapter ADD COLUMN volume_id TEXT;

CREATE TABLE story_structure (
  project_id TEXT PRIMARY KEY REFERENCES project(id) ON DELETE CASCADE,
  mode TEXT NOT NULL DEFAULT 'chapters',
  total_chapters INTEGER NOT NULL DEFAULT 0,
  spec TEXT NOT NULL DEFAULT '{}',
  updated_at TEXT NOT NULL
);

CREATE TABLE knowledge_domain (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  key TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  icon TEXT NOT NULL DEFAULT 'book',
  accent TEXT NOT NULL DEFAULT 'teal',
  is_system INTEGER NOT NULL DEFAULT 0,
  sort_order INTEGER NOT NULL DEFAULT 0,
  field_schema TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_knowledge_domain_key
  ON knowledge_domain(project_id, key);

CREATE TABLE knowledge_entry (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  domain_id TEXT NOT NULL REFERENCES knowledge_domain(id) ON DELETE CASCADE,
  entity_id TEXT REFERENCES entity(id) ON DELETE SET NULL,
  name TEXT NOT NULL,
  aliases TEXT NOT NULL DEFAULT '[]',
  summary TEXT NOT NULL DEFAULT '',
  fields TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL DEFAULT 'confirmed',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_knowledge_entry_domain
  ON knowledge_entry(project_id, domain_id, name);
CREATE INDEX idx_knowledge_entry_entity
  ON knowledge_entry(project_id, entity_id);

UPDATE project SET schema_version = 6;
