use std::collections::HashMap;

use petgraph::graph::{NodeIndex, UnGraph};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{
    domain::{
        CommunityReport, GraphData, GraphEdge, GraphNode, ImpactItem, ImpactSet,
    },
    error::{AppError, AppResult},
    now,
};

fn graph_internal(conn: &Connection, project_id: &str) -> AppResult<(UnGraph<String, String>, HashMap<String, NodeIndex>)> {
    let mut graph = UnGraph::<String, String>::new_undirected();
    let mut index = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, canonical_name FROM entity WHERE project_id=?1")?;
        let rows = stmt.query_map([project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, _name) = row?;
            let node = graph.add_node(id.clone());
            index.insert(id, node);
        }
    }
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
            let (from, to, label) = row?;
            if let (Some(&a), Some(&b)) = (index.get(&from), index.get(&to)) {
                graph.add_edge(a, b, label);
            }
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT subject_id, object_entity_id, predicate FROM claim
             WHERE project_id=?1 AND status='confirmed' AND truth_layer='world'
               AND subject_id IS NOT NULL AND object_entity_id IS NOT NULL",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (from, to, label) = row?;
            if let (Some(&a), Some(&b)) = (index.get(&from), index.get(&to)) {
                graph.add_edge(a, b, label);
            }
        }
    }
    Ok((graph, index))
}

fn detect_communities(graph: &UnGraph<String, String>) -> Vec<Vec<String>> {
    let count = graph.node_count();
    if count == 0 {
        return vec![];
    }
    let mut labels: Vec<usize> = (0..count).collect();
    for _ in 0..24 {
        let mut changed = false;
        for node in graph.node_indices() {
            let mut votes: HashMap<usize, usize> = HashMap::new();
            for neighbor in graph.neighbors(node) {
                *votes.entry(labels[neighbor.index()]).or_insert(0) += 1;
            }
            if let Some((label, _)) = votes.into_iter().max_by_key(|(_, count)| *count) {
                if labels[node.index()] != label {
                    labels[node.index()] = label;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut groups: HashMap<usize, Vec<String>> = HashMap::new();
    for node in graph.node_indices() {
        groups
            .entry(labels[node.index()])
            .or_default()
            .push(graph[node].clone());
    }
    groups.into_values().collect()
}

fn entity_name(conn: &Connection, project_id: &str, id: &str) -> AppResult<String> {
    Ok(conn.query_row(
        "SELECT canonical_name FROM entity WHERE project_id=?1 AND id=?2",
        params![project_id, id],
        |row| row.get(0),
    )?)
}

fn community_sources(conn: &Connection, project_id: &str, members: &[String]) -> AppResult<(Vec<String>, String)> {
    let mut sources = Vec::new();
    let mut lines = Vec::new();
    for member in members {
        sources.push(member.clone());
        let name = entity_name(conn, project_id, member)?;
        lines.push(format!("- 实体：{name}"));
        if let Some((domain, summary, fields)) = conn
            .query_row(
                "SELECT d.name, e.summary, e.fields
                 FROM knowledge_entry e JOIN knowledge_domain d ON d.id=e.domain_id
                 WHERE e.project_id=?1 AND e.entity_id=?2",
                params![project_id, member],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
        {
            sources.push(format!("knowledge:{member}"));
            lines.push(format!(
                "  知识域：{domain}｜{summary}｜字段：{fields}"
            ));
        }
        let mut stmt = conn.prepare(
            "SELECT id, subject, predicate, object, source_span FROM claim
             WHERE project_id=?1 AND status='confirmed' AND (subject_id=?2 OR object_entity_id=?2) LIMIT 8",
        )?;
        let rows = stmt.query_map(params![project_id, member], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        for row in rows {
            let (id, subject, predicate, object, source) = row?;
            sources.push(id);
            if !source.is_empty() {
                sources.push(source.clone());
            }
            lines.push(format!("  事实：{subject} · {predicate} · {object}（{source}）"));
        }
    }
    sources.sort();
    sources.dedup();
    Ok((sources, lines.join("\n")))
}

pub fn build_communities(conn: &Connection, project_id: &str) -> AppResult<Vec<CommunityReport>> {
    let (graph, _index) = graph_internal(conn, project_id)?;
    let groups = detect_communities(&graph);
    let ts = now();
    conn.execute("DELETE FROM graph_community WHERE project_id=?1", [project_id])?;
    let mut reports = Vec::new();
    for (order, members) in groups.into_iter().enumerate() {
        if members.len() < 2 {
            continue;
        }
        let (sources, body) = community_sources(conn, project_id, &members)?;
        let names = members
            .iter()
            .filter_map(|id| entity_name(conn, project_id, id).ok())
            .take(3)
            .collect::<Vec<_>>();
        let title = format!("关系簇 · {}", names.join(" / "));
        let content = format!(
            "派生社区报告，不是权威事实。\n成员：{}\n\n{}",
            names.join("、"),
            body
        );
        let id = uuid::Uuid::new_v4().to_string();
        let hash = crate::hash_text(&content);
        conn.execute(
            "INSERT INTO graph_community (id, project_id, level, title, member_ids, summary_hash,
                content, source_ids, generated_at) VALUES (?1,?2,0,?8,?3,?4,?5,?6,?7)",
            params![
                id,
                project_id,
                serde_json::to_string(&members)?,
                hash,
                content,
                serde_json::to_string(&sources)?,
                ts,
                title
            ],
        )?;
        reports.push(CommunityReport {
            id,
            level: 0,
            title,
            member_ids: members,
            source_ids: sources,
            content,
            summary_hash: hash,
            generated_at: ts.clone(),
            stale_at: None,
        });
        let _ = order;
    }
    Ok(reports)
}

fn community_from_row(row: &Row<'_>) -> rusqlite::Result<CommunityReport> {
    let members: String = row.get(2)?;
    let sources: String = row.get(3)?;
    Ok(CommunityReport {
        id: row.get(0)?,
        title: row.get(1)?,
        level: row.get(4)?,
        member_ids: serde_json::from_str(&members).unwrap_or_default(),
        source_ids: serde_json::from_str(&sources).unwrap_or_default(),
        content: row.get(5)?,
        summary_hash: row.get(6)?,
        generated_at: row.get(7)?,
        stale_at: row.get(8)?,
    })
}

pub fn list_communities(conn: &Connection, project_id: &str) -> AppResult<Vec<CommunityReport>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, member_ids, source_ids, level, content, summary_hash, generated_at, stale_at
         FROM graph_community WHERE project_id=?1 ORDER BY level, generated_at",
    )?;
    let rows = stmt
        .query_map([project_id], community_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

pub fn graph_data(conn: &Connection, project_id: &str) -> AppResult<GraphData> {
    let mut nodes = Vec::new();
    let mut membership = HashMap::new();
    for report in list_communities(conn, project_id)? {
        for (index, member) in report.member_ids.iter().enumerate() {
            membership.insert(member.clone(), index as i32);
        }
    }
    {
        let mut stmt = conn.prepare("SELECT id, canonical_name, type FROM entity WHERE project_id=?1")?;
        let rows = stmt.query_map([project_id], |row| {
            Ok(GraphNode {
                id: row.get(0)?,
                label: row.get(1)?,
                kind: row.get(2)?,
                community: None,
            })
        })?;
        for row in rows {
            let mut node = row?;
            node.community = membership.get(&node.id).copied();
            nodes.push(node);
        }
    }
    let mut edges = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, from_id, to_id, type FROM relation WHERE project_id=?1 AND status='confirmed'",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok(GraphEdge {
                id: row.get(0)?,
                from: row.get(1)?,
                to: row.get(2)?,
                label: row.get(3)?,
            })
        })?;
        for row in rows {
            edges.push(row?);
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT id, subject_id, object_entity_id, predicate FROM claim
             WHERE project_id=?1 AND status='confirmed' AND truth_layer='world'
               AND subject_id IS NOT NULL AND object_entity_id IS NOT NULL",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok(GraphEdge {
                id: row.get(0)?,
                from: row.get(1)?,
                to: row.get(2)?,
                label: row.get(3)?,
            })
        })?;
        for row in rows {
            edges.push(row?);
        }
    }
    Ok(GraphData { nodes, edges })
}

pub fn mark_stale(conn: &Connection, project_id: &str) -> AppResult<()> {
    let ts = now();
    conn.execute(
        "UPDATE graph_community SET stale_at=COALESCE(stale_at,?2) WHERE project_id=?1",
        params![project_id, ts],
    )?;
    conn.execute(
        "UPDATE wiki_page SET stale_at=COALESCE(stale_at,?2) WHERE project_id=?1",
        params![project_id, ts],
    )?;
    Ok(())
}

pub fn impact(conn: &Connection, project_id: &str, entity_id: &str) -> AppResult<ImpactSet> {
    let label = entity_name(conn, project_id, entity_id)?;
    let mut items = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, subject || ' · ' || predicate || ' · ' || object, source_span FROM claim
             WHERE project_id=?1 AND (subject_id=?2 OR object_entity_id=?2)",
        )?;
        let rows = stmt.query_map(params![project_id, entity_id], |row| {
            Ok(ImpactItem {
                kind: "claim".into(),
                id: row.get(0)?,
                label: row.get(1)?,
                reason: "事实直接引用该实体".into(),
                source: row.get(2)?,
            })
        })?;
        for row in rows {
            items.push(row?);
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT id, type, '' FROM relation WHERE project_id=?1 AND (from_id=?2 OR to_id=?2)",
        )?;
        let rows = stmt.query_map(params![project_id, entity_id], |row| {
            Ok(ImpactItem {
                kind: "relation".into(),
                id: row.get(0)?,
                label: row.get(1)?,
                reason: "关系边连接到该实体".into(),
                source: row.get(2)?,
            })
        })?;
        for row in rows {
            items.push(row?);
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT id, title, source_span FROM event WHERE project_id=?1 AND participants LIKE ?2",
        )?;
        let rows = stmt.query_map(params![project_id, format!("%\"{}\"%", label)], |row| {
            Ok(ImpactItem {
                kind: "event".into(),
                id: row.get(0)?,
                label: row.get(1)?,
                reason: "事件参与者包含该实体".into(),
                source: row.get(2)?,
            })
        })?;
        for row in rows {
            items.push(row?);
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT id, goal, '' FROM scene WHERE project_id=?1 AND pov=?2",
        )?;
        let rows = stmt.query_map(params![project_id, label], |row| {
            Ok(ImpactItem {
                kind: "scene".into(),
                id: row.get(0)?,
                label: row.get(1)?,
                reason: "场景 POV 指向该人物".into(),
                source: row.get(2)?,
            })
        })?;
        for row in rows {
            items.push(row?);
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT id, explanation, certificate FROM issue WHERE project_id=?1 AND claim_ids LIKE ?2",
        )?;
        let rows = stmt.query_map(params![project_id, format!("%\"{}\"%", entity_id)], |row| {
            Ok(ImpactItem {
                kind: "issue".into(),
                id: row.get(0)?,
                label: row.get(1)?,
                reason: "冲突证书引用该实体的事实".into(),
                source: row.get(2)?,
            })
        })?;
        for row in rows {
            items.push(row?);
        }
    }
    Ok(ImpactSet {
        target_id: entity_id.into(),
        target_label: label,
        items,
    })
}

pub fn insight_context(
    conn: &Connection,
    project_id: &str,
    question: &str,
) -> AppResult<(String, Vec<String>, Vec<String>)> {
    let reports = list_communities(conn, project_id)?;
    let mut selected = Vec::new();
    for report in &reports {
        if question.contains(&report.title)
            || report.content.contains(question)
            || report
                .member_ids
                .iter()
                .filter_map(|id| entity_name(conn, project_id, id).ok())
                .any(|name| question.contains(&name))
        {
            selected.push(report.clone());
        }
    }
    if selected.is_empty() {
        selected = reports.into_iter().take(5).collect();
    }
    let mut context = String::new();
    let mut sources = Vec::new();
    let mut ids = Vec::new();
    for report in &selected {
        context.push_str(&format!(
            "[社区 {}]\n{}\n来源：{}\n\n",
            report.title,
            report.content,
            report.source_ids.join(", ")
        ));
        ids.push(report.id.clone());
        sources.extend(report.source_ids.clone());
    }
    sources.sort();
    sources.dedup();
    Ok((context, sources, ids))
}

pub fn save_insight(
    conn: &Connection,
    project_id: &str,
    question: &str,
    answer: &str,
    source_ids: &[String],
    community_ids: &[String],
) -> AppResult<String> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO insight (id, project_id, question, answer, source_ids, community_ids, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            id,
            project_id,
            question,
            answer,
            serde_json::to_string(source_ids)?,
            serde_json::to_string(community_ids)?,
            now()
        ],
    )?;
    Ok(id)
}

pub fn require_entity(conn: &Connection, project_id: &str, id: &str) -> AppResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM entity WHERE project_id=?1 AND id=?2)",
        params![project_id, id],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(AppError::Message("实体不存在".into()))
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use crate::project;

    use super::*;

    #[test]
    fn communities_have_sources_and_graph_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&crate::domain::CreateProjectInput {
            name: "t".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        for (id, name) in [("a", "林澈"), ("b", "母亲"), ("c", "父亲")] {
            loaded.conn.execute(
                "INSERT INTO entity (id,project_id,type,canonical_name,aliases,status,description,created_at,updated_at)
                 VALUES (?1,?2,'character',?3,'[]','confirmed','','now','now')",
                params![id, pid, name],
            ).unwrap();
        }
        loaded.conn.execute(
            "INSERT INTO relation (id,project_id,from_id,to_id,type,status,created_at)
             VALUES ('r1',?1,'a','b','母子','confirmed','now'),
                    ('r2',?1,'b','c','夫妻','confirmed','now')",
            [&pid],
        ).unwrap();
        let reports = build_communities(&loaded.conn, &pid).unwrap();
        assert!(!reports.is_empty());
        assert!(reports.iter().all(|report| !report.source_ids.is_empty()));
        let graph = graph_data(&loaded.conn, &pid).unwrap();
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        let impact = impact(&loaded.conn, &pid, "a").unwrap();
        assert!(impact.items.iter().any(|item| item.kind == "relation"));
    }
}
