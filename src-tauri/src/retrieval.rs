use std::collections::HashMap;
use std::sync::OnceLock;

use jieba_rs::Jieba;
use rusqlite::{params, Connection};

use crate::{
    domain::{HybridSearchHit, HybridSearchResult, SearchHit},
    error::AppResult,
};

#[derive(Debug, Clone)]
pub struct SemanticQuery {
    pub vector: Vec<f32>,
    pub model: String,
    pub dimensions: i32,
}

fn jieba() -> &'static Jieba {
    static INSTANCE: OnceLock<Jieba> = OnceLock::new();
    INSTANCE.get_or_init(Jieba::new)
}

pub fn tokenize(text: &str) -> String {
    jieba()
        .cut(text, false)
        .into_iter()
        .filter(|token| !token.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn fts_query(query: &str) -> String {
    let tokens: Vec<String> = jieba()
        .cut(query, false)
        .into_iter()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{}\"", token.replace('"', "")))
        .collect();
    if tokens.is_empty() {
        "\"\"".into()
    } else {
        tokens.join(" OR ")
    }
}

pub fn search_for_scene(
    conn: &Connection,
    project_id: &str,
    query: &str,
    limit: usize,
    scene_id: Option<&str>,
) -> AppResult<Vec<SearchHit>> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT source_id, chapter_id,
            snippet(text_fts, 0, '[', ']', '…', 24),
            bm25(text_fts)
         FROM text_fts
         WHERE project_id=?1 AND tokenized MATCH ?2
         ORDER BY bm25(text_fts) ASC LIMIT ?3",
    )?;
    let raw = stmt
        .query_map(params![project_id, fts_query(query), limit as i32], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if raw.is_empty() {
        return Ok(vec![]);
    }
    let best = raw
        .iter()
        .map(|(_, _, _, rank)| *rank)
        .fold(f64::INFINITY, f64::min);
    let worst = raw
        .iter()
        .map(|(_, _, _, rank)| *rank)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = (worst - best).abs().max(0.0001);
    let preference = crate::preferences::targets(conn, project_id, scene_id)?;
    let mut ranked = raw
        .into_iter()
        .map(|(source_id, chapter_id, excerpt, rank)| {
            let lexical = if (worst - best).abs() < 0.0001 {
                100.0
            } else {
                (((worst - rank) / span) * 100.0).clamp(0.0, 100.0)
            };
            let preference_score = preference
                .as_ref()
                .map(|target| crate::preferences::score_with_target(&excerpt, target))
                .unwrap_or(0.0);
            let combined = if preference.is_some() {
                lexical * 0.8 + preference_score * 0.2
            } else {
                lexical
            };
            (
                SearchHit {
                    source_id,
                    chapter_id,
                    excerpt,
                    score: combined.round().clamp(0.0, 100.0) as i32,
                },
                combined,
            )
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.0.score.cmp(&a.0.score))
    });
    Ok(ranked.into_iter().map(|(hit, _)| hit).collect())
}

#[derive(Debug)]
struct HybridEntry {
    source_kind: String,
    source_id: String,
    chapter_id: Option<String>,
    excerpt: String,
    rrf: f64,
    fts_rank: Option<i32>,
    vector_rank: Option<i32>,
    graph_rank: Option<i32>,
}

fn hybrid_key(
    source_kind: &str,
    source_id: &str,
    chapter_id: Option<&str>,
) -> (String, String, Option<String>) {
    if source_kind == "chapter" {
        let chapter_id = chapter_id.unwrap_or(source_id);
        (
            "chapter".into(),
            chapter_id.to_string(),
            Some(chapter_id.to_string()),
        )
    } else {
        (
            source_kind.to_string(),
            source_id.to_string(),
            chapter_id.map(str::to_string),
        )
    }
}

fn add_rrf(
    entries: &mut HashMap<(String, String), HybridEntry>,
    source_kind: &str,
    source_id: &str,
    chapter_id: Option<&str>,
    excerpt: String,
    rank: usize,
    channel: &str,
) {
    let (source_kind, source_id, chapter_id) = hybrid_key(source_kind, source_id, chapter_id);
    let key = (source_kind.clone(), source_id.clone());
    let entry = entries.entry(key).or_insert_with(|| HybridEntry {
        source_kind,
        source_id,
        chapter_id,
        excerpt: String::new(),
        rrf: 0.0,
        fts_rank: None,
        vector_rank: None,
        graph_rank: None,
    });
    entry.rrf += 1.0 / (60.0 + rank as f64);
    if entry.excerpt.trim().is_empty() && !excerpt.trim().is_empty() {
        entry.excerpt = excerpt;
    }
    let rank = rank as i32;
    match channel {
        "fts" => entry.fts_rank = Some(rank),
        "vector" => entry.vector_rank = Some(rank),
        "graph" => entry.graph_rank = Some(rank),
        _ => {}
    }
}

pub fn hybrid_search(
    conn: &Connection,
    project_id: &str,
    query: &str,
    limit: usize,
    semantic: Option<&SemanticQuery>,
    embedding_error: Option<String>,
) -> AppResult<HybridSearchResult> {
    let mut entries: HashMap<(String, String), HybridEntry> = HashMap::new();
    if !query.trim().is_empty() {
        let raw = {
            let mut stmt = conn.prepare(
                "SELECT source_id, source_kind, chapter_id,
                    snippet(text_fts, 0, '[', ']', '…', 28),
                    bm25(text_fts)
                 FROM text_fts
                 WHERE project_id=?1 AND tokenized MATCH ?2
                 ORDER BY bm25(text_fts) ASC LIMIT ?3",
            )?;
            let rows = stmt
                .query_map(
                    params![project_id, fts_query(query), (limit * 3).max(12) as i32],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for (rank, (source_id, source_kind, chapter_id, excerpt)) in raw.into_iter().enumerate() {
            add_rrf(
                &mut entries,
                &source_kind,
                &source_id,
                chapter_id.as_deref(),
                excerpt,
                rank + 1,
                "fts",
            );
        }
    }

    let branch_id = crate::branches::current(conn, project_id)?;
    let mut vector_hits = Vec::new();
    if let Some(semantic) = semantic {
        vector_hits = crate::embedding::vector_search(
            conn,
            project_id,
            &semantic.model,
            semantic.dimensions,
            &semantic.vector,
            branch_id.as_deref(),
            (limit * 3).max(16),
        )?;
        for (rank, hit) in vector_hits.iter().enumerate() {
            add_rrf(
                &mut entries,
                &hit.owner_kind,
                &hit.owner_id,
                None,
                hit.content.chars().take(600).collect(),
                rank + 1,
                "vector",
            );
        }
    }

    let mut seeds = crate::embedding::link_entities(conn, project_id, query, 8)?;
    if seeds.is_empty() {
        seeds = vector_hits
            .iter()
            .filter(|hit| hit.owner_kind == "entity")
            .take(4)
            .map(|hit| hit.owner_id.clone())
            .collect();
    }
    if !seeds.is_empty() {
        let scores = ppr(conn, project_id, &seeds, 0.85)?;
        let mut ranked: Vec<_> = scores.into_iter().collect();
        ranked.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (rank, (id, _)) in ranked.into_iter().take((limit * 2).max(12)).enumerate() {
            if let Ok(entity) = crate::entities::get_entity(conn, project_id, &id) {
                add_rrf(
                    &mut entries,
                    "entity",
                    &id,
                    None,
                    format!("{}：{}", entity.canonical_name, entity.description),
                    rank + 1,
                    "graph",
                );
            }
        }
    }

    let mut rows = entries.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .rrf
            .partial_cmp(&left.rrf)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows.truncate(limit);
    let best = rows.first().map(|row| row.rrf).unwrap_or(1.0).max(0.0001);
    let has_vector = semantic.is_some() && rows.iter().any(|row| row.vector_rank.is_some());
    let hits = rows
        .into_iter()
        .map(|row| HybridSearchHit {
            source_kind: row.source_kind,
            source_id: row.source_id,
            chapter_id: row.chapter_id,
            excerpt: row.excerpt,
            score: ((row.rrf / best) * 100.0).round().clamp(0.0, 100.0) as i32,
            fts_rank: row.fts_rank,
            vector_rank: row.vector_rank,
            graph_rank: row.graph_rank,
        })
        .collect();
    Ok(HybridSearchResult {
        hits,
        retrieval_mode: if has_vector {
            "hybrid".into()
        } else {
            "fts_ppr".into()
        },
        model: semantic.map(|query| query.model.clone()),
        dimensions: semantic.map(|query| query.dimensions),
        embedding_error,
    })
}

/// 在实体图上运行 Personalized PageRank。边来自关系与同层事实中的实体连接。
pub fn ppr(
    conn: &Connection,
    project_id: &str,
    seeds: &[String],
    alpha: f64,
) -> AppResult<HashMap<String, f64>> {
    let nodes: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM entity WHERE project_id=?1")?;
        let rows = stmt
            .query_map([project_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        rows
    };
    if nodes.is_empty() {
        return Ok(HashMap::new());
    }
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let n = nodes.len();
    let mut edges: Vec<Vec<(usize, f64)>> = vec![vec![]; n];
    {
        let mut stmt = conn.prepare(
            "SELECT from_id, to_id, type FROM relation WHERE project_id=?1 AND status='confirmed'",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (from, to, kind) = row?;
            if let (Some(&a), Some(&b)) = (index.get(from.as_str()), index.get(to.as_str())) {
                let weight = if kind.contains("亲") { 2.0 } else { 1.0 };
                edges[a].push((b, weight));
                edges[b].push((a, weight));
            }
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT subject_id, object_entity_id FROM claim
             WHERE project_id=?1 AND status='confirmed' AND truth_layer='world'
               AND subject_id IS NOT NULL AND object_entity_id IS NOT NULL",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (from, to) = row?;
            if let (Some(&a), Some(&b)) = (index.get(from.as_str()), index.get(to.as_str())) {
                edges[a].push((b, 0.5));
                edges[b].push((a, 0.5));
            }
        }
    }

    let mut personal = vec![0.0; n];
    let mut seed_count = 0;
    for seed in seeds {
        if let Some(&i) = index.get(seed.as_str()) {
            personal[i] = 1.0;
            seed_count += 1;
        }
    }
    if seed_count == 0 {
        for value in personal.iter_mut() {
            *value = 1.0 / n as f64;
        }
    } else {
        for value in personal.iter_mut() {
            *value /= seed_count as f64;
        }
    }

    let mut rank = personal.clone();
    for _ in 0..24 {
        let mut next = vec![0.0; n];
        for (i, neighbors) in edges.iter().enumerate() {
            let total: f64 = neighbors.iter().map(|(_, weight)| weight).sum();
            if total <= 0.0 {
                continue;
            }
            for (j, weight) in neighbors {
                next[*j] += rank[i] * weight / total;
            }
        }
        for i in 0..n {
            rank[i] = (1.0 - alpha) * personal[i] + alpha * next[i];
        }
    }
    let sum: f64 = rank.iter().sum();
    if sum > 0.0 {
        for value in rank.iter_mut() {
            *value /= sum;
        }
    }
    Ok(nodes
        .into_iter()
        .zip(rank)
        .collect::<HashMap<String, f64>>())
}

pub fn entity_id_by_name(
    conn: &Connection,
    project_id: &str,
    name: &str,
) -> AppResult<Option<String>> {
    if name.trim().is_empty() {
        return Ok(None);
    }
    Ok(conn
        .query_row(
            "SELECT id FROM entity WHERE project_id=?1 AND (canonical_name=?2 OR aliases LIKE ?3) LIMIT 1",
            params![project_id, name, format!("%\"{}\"%", name)],
            |row| row.get(0),
        )
        .ok())
}

pub fn graph_claims(
    conn: &Connection,
    project_id: &str,
    entity_ids: &[String],
    limit: usize,
) -> AppResult<Vec<(String, String, String)>> {
    if entity_ids.is_empty() {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT id, subject || ' · ' || predicate || ' · ' || object, source_span FROM claim
         WHERE project_id=?1 AND status='confirmed' AND (subject_id=?2 OR object_entity_id=?2)
         LIMIT ?3",
    )?;
    let mut result = Vec::new();
    for id in entity_ids {
        let rows = stmt.query_map(params![project_id, id, limit as i32], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            result.push(row?);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_chinese_into_searchable_terms() {
        let tokens = tokenize("林澈在雨夜握紧旧钥匙");
        assert!(tokens.contains("林澈") || tokens.contains("雨夜"));
        assert!(tokens.split(' ').count() >= 3);
    }

    #[test]
    fn ppr_prefers_neighbors_of_seed() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut conn).unwrap();
        conn.execute_batch(
            "INSERT INTO project (id,name,root_path,schema_version,writing_language,mode,created_at,updated_at)
             VALUES ('p','t','/tmp/t',1,'zh-CN','idea','now','now');
             INSERT INTO entity (id,project_id,type,canonical_name,aliases,status,description,created_at,updated_at)
             VALUES ('a','p','character','A','[]','confirmed','','now','now'),
                    ('b','p','character','B','[]','confirmed','','now','now'),
                    ('c','p','character','C','[]','confirmed','','now','now');
             INSERT INTO relation (id,project_id,from_id,to_id,type,status,created_at)
             VALUES ('ab','p','a','b','friend','confirmed','now'),
                    ('bc','p','b','c','friend','confirmed','now');",
        )
        .unwrap();
        let scores = ppr(&conn, "p", &["a".into()], 0.85).unwrap();
        assert!(scores["b"] > scores["c"]);
        assert!(scores["a"] > 0.0);
    }

    #[test]
    fn hybrid_search_fuses_fts_vector_and_graph_with_rrf() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut conn).unwrap();
        conn.execute_batch(
            "INSERT INTO project (id,name,root_path,schema_version,writing_language,mode,created_at,updated_at)
             VALUES ('p','t','/tmp/t',9,'zh-CN','idea','now','now');
             INSERT INTO entity (id,project_id,type,canonical_name,aliases,status,description,created_at,updated_at)
             VALUES ('a','p','character','林澈','[]','confirmed','寻找旧钥匙','now','now'),
                    ('b','p','character','守夜人','[]','confirmed','保管旧钥匙','now','now');
             INSERT INTO relation (id,project_id,from_id,to_id,type,status,created_at)
             VALUES ('ab','p','a','b','敌对','confirmed','now');",
        )
        .unwrap();
        let content = "林澈握紧旧钥匙，守夜人看见后退了一步。";
        conn.execute(
            "INSERT INTO text_fts
             (content, tokenized, source_id, project_id, source_kind, chapter_id, branch_id)
             VALUES (?1,?2,'a','p','entity',NULL,NULL)",
            params![content, tokenize(content)],
        )
        .unwrap();
        conn.execute_batch(
            "INSERT INTO embedding
             (id,project_id,owner_kind,owner_id,model,dim,vector,content,content_hash,status,
              branch_id,updated_at,created_at)
             VALUES ('ea','p','entity','a','embed',2,X'0000803F00000000','阿澈握着黄铜钥匙','ha','ready',NULL,'now','now'),
                    ('eb','p','entity','b','embed',2,X'0000803F00000000','守夜人保管钥匙','hb','ready',NULL,'now','now');",
        )
        .unwrap();
        let semantic = SemanticQuery {
            vector: vec![1.0, 0.0],
            model: "embed".into(),
            dimensions: 2,
        };
        let result = hybrid_search(&conn, "p", "林澈与旧钥匙", 10, Some(&semantic), None).unwrap();
        assert_eq!(result.retrieval_mode, "hybrid");
        let entity = result.hits.iter().find(|hit| hit.source_id == "a").unwrap();
        assert!(entity.fts_rank.is_some());
        assert!(entity.vector_rank.is_some());
        assert!(entity.graph_rank.is_some());
        assert!(result
            .hits
            .iter()
            .any(|hit| hit.source_id == "b" && hit.vector_rank.is_some()));
    }

    #[test]
    fn hybrid_search_degrades_to_fts_and_ppr_without_query_vector() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut conn).unwrap();
        conn.execute_batch(
            "INSERT INTO project (id,name,root_path,schema_version,writing_language,mode,created_at,updated_at)
             VALUES ('p','t','/tmp/t',9,'zh-CN','idea','now','now');
             INSERT INTO entity (id,project_id,type,canonical_name,aliases,status,description,created_at,updated_at)
             VALUES ('a','p','character','林澈','[]','confirmed','寻找旧钥匙','now','now');",
        )
        .unwrap();
        let content = "林澈在旧车站找到旧钥匙。";
        conn.execute(
            "INSERT INTO text_fts
             (content, tokenized, source_id, project_id, source_kind, chapter_id, branch_id)
             VALUES (?1,?2,'a','p','entity',NULL,NULL)",
            params![content, tokenize(content)],
        )
        .unwrap();
        let result = hybrid_search(
            &conn,
            "p",
            "旧钥匙",
            10,
            None,
            Some("Embedding 服务不可用".into()),
        )
        .unwrap();
        assert_eq!(result.retrieval_mode, "fts_ppr");
        assert_eq!(
            result.embedding_error.as_deref(),
            Some("Embedding 服务不可用")
        );
        assert!(!result.hits.is_empty());
        assert!(result.hits.iter().all(|hit| hit.vector_rank.is_none()));
    }
}
