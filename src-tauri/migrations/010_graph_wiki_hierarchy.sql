ALTER TABLE graph_community ADD COLUMN parent_id TEXT;
ALTER TABLE graph_community ADD COLUMN community_key TEXT NOT NULL DEFAULT '';
ALTER TABLE graph_community ADD COLUMN source_hash TEXT NOT NULL DEFAULT '';
ALTER TABLE graph_community ADD COLUMN summary_mode TEXT NOT NULL DEFAULT 'deterministic';

UPDATE graph_community
SET community_key = id,
    source_hash = summary_hash
WHERE community_key = '';

CREATE UNIQUE INDEX idx_graph_community_key
  ON graph_community(project_id, community_key);
CREATE INDEX idx_graph_community_parent
  ON graph_community(project_id, parent_id);

ALTER TABLE wiki_page ADD COLUMN page_key TEXT NOT NULL DEFAULT '';
ALTER TABLE wiki_page ADD COLUMN source_hash TEXT NOT NULL DEFAULT '';
ALTER TABLE wiki_page ADD COLUMN compile_mode TEXT NOT NULL DEFAULT 'deterministic';

UPDATE wiki_page
SET page_key = id,
    source_hash = content_hash
WHERE page_key = '';

CREATE UNIQUE INDEX idx_wiki_page_key
  ON wiki_page(project_id, page_key);

CREATE TABLE wiki_page_dependency (
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  page_id TEXT NOT NULL REFERENCES wiki_page(id) ON DELETE CASCADE,
  depends_on_page_id TEXT NOT NULL REFERENCES wiki_page(id) ON DELETE CASCADE,
  reason TEXT NOT NULL,
  PRIMARY KEY (page_id, depends_on_page_id)
);
CREATE INDEX idx_wiki_dependency_target
  ON wiki_page_dependency(project_id, depends_on_page_id);

UPDATE project SET schema_version = 10;
