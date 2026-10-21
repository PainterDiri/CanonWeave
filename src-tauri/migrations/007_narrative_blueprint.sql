ALTER TABLE premise ADD COLUMN dramatic_question TEXT NOT NULL DEFAULT '';
ALTER TABLE premise ADD COLUMN antagonistic_force TEXT NOT NULL DEFAULT '';
ALTER TABLE premise ADD COLUMN world_rule TEXT NOT NULL DEFAULT '';
ALTER TABLE premise ADD COLUMN ending_direction TEXT NOT NULL DEFAULT '';
ALTER TABLE outline_node ADD COLUMN logic_node_ids TEXT NOT NULL DEFAULT '[]';
ALTER TABLE outline_node ADD COLUMN presentation_mode TEXT NOT NULL DEFAULT 'prose';
ALTER TABLE outline_node ADD COLUMN narrative_function TEXT NOT NULL DEFAULT '';

CREATE TABLE narrative_blueprint (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  premise_id TEXT REFERENCES premise(id) ON DELETE SET NULL,
  title TEXT NOT NULL,
  medium TEXT NOT NULL DEFAULT 'novel',
  structure_strategy TEXT NOT NULL DEFAULT 'adaptive',
  target_units INTEGER,
  core_dramatic_question TEXT NOT NULL DEFAULT '',
  narrative_promise TEXT NOT NULL DEFAULT '',
  opening_state TEXT NOT NULL DEFAULT '',
  ending_state TEXT NOT NULL DEFAULT '',
  theme_question TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'candidate',
  completeness INTEGER NOT NULL DEFAULT 0,
  diagnostics TEXT NOT NULL DEFAULT '[]',
  version INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_narrative_blueprint_project
  ON narrative_blueprint(project_id, status, updated_at);
CREATE UNIQUE INDEX idx_narrative_blueprint_confirmed
  ON narrative_blueprint(project_id) WHERE status='confirmed';

CREATE TABLE narrative_element (
  id TEXT PRIMARY KEY,
  blueprint_id TEXT NOT NULL REFERENCES narrative_blueprint(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  name TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT '',
  summary TEXT NOT NULL DEFAULT '',
  external_goal TEXT NOT NULL DEFAULT '',
  internal_need TEXT NOT NULL DEFAULT '',
  fear TEXT NOT NULL DEFAULT '',
  flaw TEXT NOT NULL DEFAULT '',
  leverage TEXT NOT NULL DEFAULT '',
  constraints_json TEXT NOT NULL DEFAULT '[]',
  secret TEXT NOT NULL DEFAULT '',
  relations_json TEXT NOT NULL DEFAULT '[]',
  metadata_json TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL DEFAULT 'candidate',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_narrative_element_blueprint
  ON narrative_element(blueprint_id, kind, sort_order);

CREATE TABLE narrative_timeline_event (
  id TEXT PRIMARY KEY,
  blueprint_id TEXT NOT NULL REFERENCES narrative_blueprint(id) ON DELETE CASCADE,
  order_no INTEGER NOT NULL,
  title TEXT NOT NULL,
  story_time TEXT NOT NULL DEFAULT '',
  cause TEXT NOT NULL DEFAULT '',
  participants_json TEXT NOT NULL DEFAULT '[]',
  outcome TEXT NOT NULL DEFAULT '',
  consequence TEXT NOT NULL DEFAULT '',
  reveal_at TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'candidate',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_narrative_timeline_blueprint
  ON narrative_timeline_event(blueprint_id, order_no);

CREATE TABLE narrative_thread (
  id TEXT PRIMARY KEY,
  blueprint_id TEXT NOT NULL REFERENCES narrative_blueprint(id) ON DELETE CASCADE,
  order_no INTEGER NOT NULL,
  kind TEXT NOT NULL DEFAULT 'main',
  title TEXT NOT NULL,
  dramatic_question TEXT NOT NULL DEFAULT '',
  opening_state TEXT NOT NULL DEFAULT '',
  target_state TEXT NOT NULL DEFAULT '',
  stakes TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'candidate',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_narrative_thread_blueprint
  ON narrative_thread(blueprint_id, order_no);

CREATE TABLE narrative_logic_node (
  id TEXT PRIMARY KEY,
  blueprint_id TEXT NOT NULL REFERENCES narrative_blueprint(id) ON DELETE CASCADE,
  thread_id TEXT REFERENCES narrative_thread(id) ON DELETE SET NULL,
  order_no INTEGER NOT NULL,
  kind TEXT NOT NULL,
  title TEXT NOT NULL,
  cause TEXT NOT NULL DEFAULT '',
  choice TEXT NOT NULL DEFAULT '',
  cost TEXT NOT NULL DEFAULT '',
  outcome TEXT NOT NULL DEFAULT '',
  state_change TEXT NOT NULL DEFAULT '',
  knowledge_change TEXT NOT NULL DEFAULT '',
  participants_json TEXT NOT NULL DEFAULT '[]',
  location TEXT NOT NULL DEFAULT '',
  story_time TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'candidate',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_narrative_logic_node_blueprint
  ON narrative_logic_node(blueprint_id, order_no);

CREATE TABLE narrative_logic_edge (
  id TEXT PRIMARY KEY,
  blueprint_id TEXT NOT NULL REFERENCES narrative_blueprint(id) ON DELETE CASCADE,
  from_node_id TEXT NOT NULL REFERENCES narrative_logic_node(id) ON DELETE CASCADE,
  to_node_id TEXT NOT NULL REFERENCES narrative_logic_node(id) ON DELETE CASCADE,
  relation TEXT NOT NULL,
  condition TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'candidate',
  created_at TEXT NOT NULL
);
CREATE INDEX idx_narrative_logic_edge_blueprint
  ON narrative_logic_edge(blueprint_id, from_node_id, to_node_id);

CREATE TABLE narrative_presentation (
  id TEXT PRIMARY KEY,
  blueprint_id TEXT NOT NULL REFERENCES narrative_blueprint(id) ON DELETE CASCADE,
  logic_node_id TEXT NOT NULL REFERENCES narrative_logic_node(id) ON DELETE CASCADE,
  medium TEXT NOT NULL,
  mode TEXT NOT NULL,
  pov TEXT NOT NULL DEFAULT '',
  dialogue_ratio INTEGER NOT NULL DEFAULT 0,
  narration_ratio INTEGER NOT NULL DEFAULT 0,
  player_agency TEXT NOT NULL DEFAULT '',
  visual_requirements TEXT NOT NULL DEFAULT '',
  audio_requirements TEXT NOT NULL DEFAULT '',
  interaction TEXT NOT NULL DEFAULT '',
  deliverable TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'candidate',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_narrative_presentation_node
  ON narrative_presentation(blueprint_id, logic_node_id);

UPDATE project SET schema_version = 7;
