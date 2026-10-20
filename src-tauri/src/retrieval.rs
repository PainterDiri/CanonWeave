use std::collections::HashMap;
use std::sync::OnceLock;

use jieba_rs::Jieba;
use rusqlite::{params, Connection};

use crate::{
    domain::SearchHit,
    error::AppResult,
};

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

pub fn search(
    conn: &Connection,
    project_id: &str,
    query: &str,
    limit: usize,
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
        .query_map(
            params![project_id, fts_query(query), limit as i32],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, f64>(3)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if raw.is_empty() {
        return Ok(vec![]);
    }
    let best = raw.iter().map(|(_, _, _, rank)| *rank).fold(f64::INFINITY, f64::min);
    let worst = raw.iter().map(|(_, _, _, rank)| *rank).fold(f64::NEG_INFINITY, f64::max);
    let span = (worst - best).abs().max(0.0001);
    Ok(raw
        .into_iter()
        .map(|(source_id, chapter_id, excerpt, rank)| SearchHit {
            source_id,
            chapter_id,
            excerpt,
            score: (((worst - rank) / span) * 100.0).round().clamp(0.0, 100.0) as i32,
        })
        .collect())
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
}
