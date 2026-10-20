ALTER TABLE wiki_page ADD COLUMN source_spans TEXT NOT NULL DEFAULT '[]';
ALTER TABLE wiki_page ADD COLUMN generator_version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE wiki_page ADD COLUMN edited_by_author INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_wiki_project_entity ON wiki_page(project_id, entity_id, kind);

CREATE TABLE character_voice_sample (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  character_id TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  text TEXT NOT NULL,
  source_span TEXT NOT NULL,
  source_type TEXT NOT NULL DEFAULT 'author',
  approved INTEGER NOT NULL DEFAULT 1,
  metrics TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);
CREATE INDEX idx_voice_sample_character
  ON character_voice_sample(project_id, character_id, approved, created_at);

CREATE TABLE character_voice_profile (
  character_id TEXT PRIMARY KEY REFERENCES entity(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL,
  centroid TEXT NOT NULL DEFAULT '{}',
  thresholds TEXT NOT NULL DEFAULT '{}',
  sample_count INTEGER NOT NULL DEFAULT 0,
  version INTEGER NOT NULL DEFAULT 1,
  updated_at TEXT NOT NULL
);

ALTER TABLE promise ADD COLUMN plant_evidence_ids TEXT NOT NULL DEFAULT '[]';
ALTER TABLE promise ADD COLUMN payoff_evidence_ids TEXT NOT NULL DEFAULT '[]';
ALTER TABLE promise ADD COLUMN payoff_state_change TEXT NOT NULL DEFAULT '';
