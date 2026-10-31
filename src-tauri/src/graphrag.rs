use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use petgraph::graph::{NodeIndex, UnGraph};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{
    domain::{CommunityReport, GraphData, GraphEdge, GraphNode, ImpactItem, ImpactSet},
    error::{AppError, AppResult},
    now,
    retrieval::{hybrid_search, SemanticQuery},
};

fn graph_internal(
    conn: &Connection,
    project_id: &str,
) -> AppResult<(UnGraph<String, String>, HashMap<String, NodeIndex>)> {
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

#[derive(Debug, Clone)]
struct HierarchicalCommunity {
    key: String,
    parent_key: Option<String>,
    level: i32,
    members: Vec<String>,
}

fn weighted_neighbors(graph: &UnGraph<String, String>) -> BTreeMap<String, BTreeMap<String, f64>> {
    let mut adjacency = BTreeMap::<String, BTreeMap<String, f64>>::new();
    for node in graph.node_indices() {
        adjacency.entry(graph[node].clone()).or_default();
    }
    for edge in graph.edge_indices() {
        let Some((left, right)) = graph.edge_endpoints(edge) else {
            continue;
        };
        let left_id = graph[left].clone();
        let right_id = graph[right].clone();
        if left_id == right_id {
            continue;
        }
        *adjacency
            .entry(left_id.clone())
            .or_default()
            .entry(right_id.clone())
            .or_insert(0.0) += 1.0;
        *adjacency
            .entry(right_id)
            .or_default()
            .entry(left_id)
            .or_insert(0.0) += 1.0;
    }
    adjacency
}

fn agglomerate(
    nodes: &[String],
    adjacency: &BTreeMap<String, BTreeMap<String, f64>>,
    threshold: f64,
) -> Vec<Vec<String>> {
    let mut groups = nodes
        .iter()
        .map(|node| vec![node.clone()])
        .collect::<Vec<_>>();
    loop {
        let mut best: Option<(f64, usize, usize)> = None;
        for left in 0..groups.len() {
            for right in (left + 1)..groups.len() {
                let mut cross_weight = 0.0;
                for from in &groups[left] {
                    for to in &groups[right] {
                        cross_weight += adjacency
                            .get(from)
                            .and_then(|neighbors| neighbors.get(to))
                            .copied()
                            .unwrap_or(0.0);
                    }
                }
                let density = cross_weight / (groups[left].len() * groups[right].len()) as f64;
                if density + f64::EPSILON < threshold {
                    continue;
                }
                let replace = best
                    .map(|(best_score, best_left, best_right)| {
                        density > best_score + f64::EPSILON
                            || ((density - best_score).abs() <= f64::EPSILON
                                && (left, right) < (best_left, best_right))
                    })
                    .unwrap_or(true);
                if replace {
                    best = Some((density, left, right));
                }
            }
        }
        let Some((_, left, right)) = best else {
            break;
        };
        let moved = groups.remove(right);
        groups[left].extend(moved);
        groups[left].sort();
        groups[left].dedup();
        groups.sort();
    }
    groups
}

fn community_key(level: i32, members: &[String]) -> String {
    crate::hash_text(&format!(
        "community:{level}:{}",
        serde_json::to_string(members).unwrap_or_default()
    ))
}

fn aggregate_adjacency(
    groups: &[HierarchicalCommunity],
    adjacency: &BTreeMap<String, BTreeMap<String, f64>>,
) -> BTreeMap<String, BTreeMap<String, f64>> {
    let mut node_to_group = BTreeMap::<String, String>::new();
    for group in groups {
        for member in &group.members {
            node_to_group.insert(member.clone(), group.key.clone());
        }
    }
    let mut aggregated = groups
        .iter()
        .map(|group| (group.key.clone(), BTreeMap::new()))
        .collect::<BTreeMap<_, _>>();
    for (from, neighbors) in adjacency {
        let Some(from_group) = node_to_group.get(from) else {
            continue;
        };
        for (to, weight) in neighbors {
            let Some(to_group) = node_to_group.get(to) else {
                continue;
            };
            if from_group == to_group {
                continue;
            }
            *aggregated
                .entry(from_group.clone())
                .or_default()
                .entry(to_group.clone())
                .or_insert(0.0) += *weight;
        }
    }
    aggregated
}

fn detect_community_hierarchy(graph: &UnGraph<String, String>) -> Vec<HierarchicalCommunity> {
    let adjacency = weighted_neighbors(graph);
    let nodes = adjacency.keys().cloned().collect::<Vec<_>>();
    if nodes.is_empty() {
        return vec![];
    }
    let mut current = agglomerate(&nodes, &adjacency, 0.5)
        .into_iter()
        .map(|members| HierarchicalCommunity {
            key: community_key(0, &members),
            parent_key: None,
            level: 0,
            members,
        })
        .collect::<Vec<_>>();
    let mut all = current.clone();

    for (level, threshold) in [(1, 0.08), (2, 0.02), (3, 0.001), (4, 0.0001)] {
        if current.len() < 2 {
            break;
        }
        let aggregated = aggregate_adjacency(&current, &adjacency);
        let group_nodes = aggregated.keys().cloned().collect::<Vec<_>>();
        let parent_label_groups = agglomerate(&group_nodes, &aggregated, threshold);
        if parent_label_groups.iter().all(|group| group.len() < 2) {
            break;
        }
        let key_to_group = current
            .iter()
            .cloned()
            .map(|group| (group.key.clone(), group))
            .collect::<BTreeMap<_, _>>();
        let mut parents = Vec::new();
        for child_keys in parent_label_groups {
            if child_keys.len() < 2 {
                continue;
            }
            let mut members = Vec::new();
            for child_key in &child_keys {
                if let Some(child) = key_to_group.get(child_key) {
                    members.extend(child.members.clone());
                }
            }
            members.sort();
            members.dedup();
            let key = community_key(level, &members);
            parents.push(HierarchicalCommunity {
                key: key.clone(),
                parent_key: None,
                level,
                members,
            });
            for child in &mut current {
                if child_keys.contains(&child.key) {
                    child.parent_key = Some(key.clone());
                    if let Some(existing) = all.iter_mut().find(|item| item.key == child.key) {
                        existing.parent_key = Some(key.clone());
                    }
                }
            }
        }
        if parents.is_empty() {
            break;
        }
        all.extend(parents.clone());
        current = parents;
    }
    all.sort_by(|left, right| {
        left.level
            .cmp(&right.level)
            .then_with(|| left.key.cmp(&right.key))
    });
    all
}

fn entity_name(conn: &Connection, project_id: &str, id: &str) -> AppResult<String> {
    Ok(conn.query_row(
        "SELECT canonical_name FROM entity WHERE project_id=?1 AND id=?2",
        params![project_id, id],
        |row| row.get(0),
    )?)
}

fn community_sources(
    conn: &Connection,
    project_id: &str,
    members: &[String],
) -> AppResult<(Vec<String>, String)> {
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
            lines.push(format!("  知识域：{domain}｜{summary}｜字段：{fields}"));
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
            lines.push(format!(
                "  事实：{subject} · {predicate} · {object}（{source}）"
            ));
        }
    }
    sources.sort();
    sources.dedup();
    Ok((sources, lines.join("\n")))
}

pub fn build_communities(conn: &Connection, project_id: &str) -> AppResult<Vec<CommunityReport>> {
    crate::embedding::delete_kind_embeddings(conn, project_id, "community")?;
    let (graph, _index) = graph_internal(conn, project_id)?;
    let groups = detect_community_hierarchy(&graph);
    let existing = list_communities(conn, project_id)?
        .into_iter()
        .map(|report| (report.community_key.clone(), report))
        .collect::<HashMap<_, _>>();
    let ts = now();
    let tx = conn.unchecked_transaction()?;
    let mut reports = Vec::new();
    for group in groups {
        if group.members.len() < 2 {
            continue;
        }
        let (sources, body) = community_sources(conn, project_id, &group.members)?;
        let source_hash =
            crate::hash_text(&format!("{}\n{}", serde_json::to_string(&sources)?, body));
        let existing_report = existing.get(&group.key);
        if let Some(report) = existing_report {
            if report.source_hash == source_hash && report.stale_at.is_none() {
                tx.execute(
                    "UPDATE graph_community SET parent_id=?3 WHERE project_id=?1 AND id=?2",
                    params![project_id, report.id, group.parent_key],
                )?;
                let mut retained = report.clone();
                retained.parent_id = group.parent_key.clone();
                reports.push(retained);
                continue;
            }
        }
        let names = group
            .members
            .iter()
            .filter_map(|id| entity_name(conn, project_id, id).ok())
            .take(3)
            .collect::<Vec<_>>();
        let title = if group.level == 0 {
            format!("关系簇 · {}", names.join(" / "))
        } else {
            format!("叙事网络 L{} · {}", group.level, names.join(" / "))
        };
        let content = format!(
            "派生社区报告，不是权威事实。\n成员：{}\n\n{}",
            names.join("、"),
            body
        );
        let id = existing_report
            .map(|report| report.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let hash = crate::hash_text(&content);
        tx.execute(
            "INSERT INTO graph_community
                (id, project_id, level, title, member_ids, summary_hash, content, source_ids,
                 generated_at, parent_id, community_key, source_hash, summary_mode, stale_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'deterministic',NULL)
             ON CONFLICT(id) DO UPDATE SET
               level=excluded.level,
               title=excluded.title,
               member_ids=excluded.member_ids,
               summary_hash=excluded.summary_hash,
               content=excluded.content,
               source_ids=excluded.source_ids,
               generated_at=excluded.generated_at,
               parent_id=excluded.parent_id,
               community_key=excluded.community_key,
               source_hash=excluded.source_hash,
               summary_mode=excluded.summary_mode,
               stale_at=NULL",
            params![
                id,
                project_id,
                group.level,
                title,
                serde_json::to_string(&group.members)?,
                hash,
                content,
                serde_json::to_string(&sources)?,
                ts,
                group.parent_key,
                group.key,
                source_hash
            ],
        )?;
        reports.push(CommunityReport {
            id,
            level: group.level,
            parent_id: group.parent_key,
            community_key: group.key,
            title,
            member_ids: group.members,
            source_ids: sources,
            content,
            summary_hash: hash,
            source_hash,
            summary_mode: "deterministic".into(),
            generated_at: ts.clone(),
            stale_at: None,
        });
    }
    let current_ids = reports
        .iter()
        .map(|report| report.id.as_str())
        .collect::<Vec<_>>();
    if current_ids.is_empty() {
        tx.execute(
            "DELETE FROM graph_community WHERE project_id=?1",
            [project_id],
        )?;
    } else {
        let placeholders = std::iter::repeat("?")
            .take(current_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "DELETE FROM graph_community WHERE project_id=?1 AND id NOT IN ({placeholders})"
        );
        let mut values = vec![project_id.to_string()];
        values.extend(current_ids.into_iter().map(str::to_string));
        tx.execute(&sql, rusqlite::params_from_iter(values))?;
    }
    tx.commit()?;
    reports.sort_by(|left, right| {
        left.level
            .cmp(&right.level)
            .then_with(|| left.title.cmp(&right.title))
    });
    Ok(reports)
}

fn community_from_row(row: &Row<'_>) -> rusqlite::Result<CommunityReport> {
    let members: String = row.get(2)?;
    let sources: String = row.get(3)?;
    Ok(CommunityReport {
        id: row.get(0)?,
        title: row.get(1)?,
        member_ids: serde_json::from_str(&members).unwrap_or_default(),
        source_ids: serde_json::from_str(&sources).unwrap_or_default(),
        level: row.get(4)?,
        content: row.get(5)?,
        summary_hash: row.get(6)?,
        generated_at: row.get(7)?,
        stale_at: row.get(8)?,
        parent_id: row.get(9)?,
        community_key: row.get(10)?,
        source_hash: row.get(11)?,
        summary_mode: row.get(12)?,
    })
}

pub fn list_communities(conn: &Connection, project_id: &str) -> AppResult<Vec<CommunityReport>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, member_ids, source_ids, level, content, summary_hash, generated_at,
                stale_at, parent_id, community_key, source_hash, summary_mode
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
    let mut community_index = 0;
    for report in list_communities(conn, project_id)?
        .into_iter()
        .filter(|report| report.level == 0)
    {
        for member in &report.member_ids {
            membership.insert(member.clone(), community_index);
        }
        community_index += 1;
    }
    {
        let mut stmt =
            conn.prepare("SELECT id, canonical_name, type FROM entity WHERE project_id=?1")?;
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
    crate::embedding::mark_changed_stale(conn, project_id)?;
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
        let mut stmt =
            conn.prepare("SELECT id, goal, '' FROM scene WHERE project_id=?1 AND pov=?2")?;
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

#[derive(Debug, Clone)]
pub struct InsightContext {
    pub context: String,
    pub source_ids: Vec<String>,
    pub community_ids: Vec<String>,
    pub entity_ids: Vec<String>,
    pub retrieval_mode: String,
    pub embedding_error: Option<String>,
}

pub struct CommunitySummaryInput {
    pub report: CommunityReport,
    pub evidence: String,
    pub source_ids: Vec<String>,
}

fn cited_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find(']') else {
            break;
        };
        for id in rest[..end].split([',', '，', ';', '；', ' ']) {
            let id = id.trim().trim_matches(['"', '\'', '`']);
            if !id.is_empty() {
                ids.push(id.to_string());
            }
        }
        rest = &rest[end + 1..];
    }
    ids.sort();
    ids.dedup();
    ids
}

pub fn validate_grounded_summary(text: &str, source_ids: &[String]) -> AppResult<Vec<String>> {
    let allowed = source_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let cited = cited_ids(text);
    if cited.is_empty() {
        return Err(AppError::Message(
            "模型摘要没有引用来源 ID，结果未写入派生社区报告".into(),
        ));
    }
    let unknown = cited
        .iter()
        .filter(|id| !allowed.contains(id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(AppError::Message(format!(
            "模型摘要引用了未提供的来源：{}",
            unknown.join("、")
        )));
    }
    Ok(cited)
}

pub fn community_summary_input(
    conn: &Connection,
    project_id: &str,
    community_id: &str,
) -> AppResult<CommunitySummaryInput> {
    let report = conn
        .query_row(
            "SELECT id, title, member_ids, source_ids, level, content, summary_hash,
                    generated_at, stale_at, parent_id, community_key, source_hash, summary_mode
             FROM graph_community WHERE project_id=?1 AND id=?2",
            params![project_id, community_id],
            community_from_row,
        )
        .optional()?
        .ok_or_else(|| AppError::Message("找不到社区报告".into()))?;
    let (source_ids, evidence) = community_sources(conn, project_id, &report.member_ids)?;
    Ok(CommunitySummaryInput {
        report,
        evidence,
        source_ids,
    })
}

pub fn apply_llm_summary(
    conn: &Connection,
    project_id: &str,
    community_id: &str,
    expected_source_hash: &str,
    summary: &str,
    cited_ids: &[String],
) -> AppResult<()> {
    let report = conn
        .query_row(
            "SELECT source_hash, source_ids FROM graph_community
             WHERE project_id=?1 AND id=?2",
            params![project_id, community_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::Message("社区报告已在摘要完成前被删除".into()))?;
    if report.0 != expected_source_hash {
        return Err(AppError::Message(
            "社区来源已变化，本次模型摘要已丢弃；请重新运行社区构建".into(),
        ));
    }
    let content = format!(
        "派生社区报告，不是权威事实。由模型基于受控来源整理；来源引用：{}\n\n{}",
        cited_ids.join("、"),
        summary.trim()
    );
    let summary_hash = crate::hash_text(&content);
    conn.execute(
        "UPDATE graph_community
         SET content=?3, summary_hash=?4, summary_mode='llm', generated_at=?5, stale_at=NULL
         WHERE project_id=?1 AND id=?2",
        params![project_id, community_id, content, summary_hash, now()],
    )?;
    Ok(())
}

pub fn insight_context(
    conn: &Connection,
    project_id: &str,
    question: &str,
    semantic: Option<&SemanticQuery>,
    embedding_error: Option<String>,
) -> AppResult<InsightContext> {
    let hybrid = hybrid_search(conn, project_id, question, 14, semantic, embedding_error)?;
    let mut seeds = crate::embedding::link_entities(conn, project_id, question, 8)?;
    seeds.extend(
        hybrid
            .hits
            .iter()
            .filter(|hit| hit.source_kind == "entity")
            .map(|hit| hit.source_id.clone()),
    );
    seeds.sort();
    seeds.dedup();
    let scores = if seeds.is_empty() {
        HashMap::new()
    } else {
        crate::retrieval::ppr(conn, project_id, &seeds, 0.85)?
    };
    let mut ranked_entities = scores.into_iter().collect::<Vec<_>>();
    ranked_entities.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    let mut entity_ids = seeds.clone();
    entity_ids.extend(ranked_entities.iter().take(18).map(|(id, _)| id.clone()));
    entity_ids.sort();
    entity_ids.dedup();
    let graph_scores = ranked_entities.into_iter().collect::<HashMap<_, _>>();
    let hit_sources = hybrid
        .hits
        .iter()
        .map(|hit| hit.source_id.as_str())
        .collect::<HashSet<_>>();

    let reports = list_communities(conn, project_id)?;
    let all_reports = reports.clone();
    let mut scored_reports = reports
        .into_iter()
        .filter_map(|report| {
            let member_overlap = report
                .member_ids
                .iter()
                .filter(|id| entity_ids.contains(id))
                .count() as f64;
            let source_overlap = report
                .source_ids
                .iter()
                .filter(|id| hit_sources.contains(id.as_str()))
                .count() as f64;
            let graph_score = report
                .member_ids
                .iter()
                .filter_map(|id| graph_scores.get(id))
                .sum::<f64>()
                * 100.0;
            let lexical_names = report
                .member_ids
                .iter()
                .filter_map(|id| entity_name(conn, project_id, id).ok())
                .filter(|name| question.contains(name))
                .count() as f64;
            let level_bonus = if report.level == 0 { 0.35 } else { 0.0 };
            let score = member_overlap * 2.0
                + source_overlap * 3.0
                + graph_score
                + lexical_names * 2.0
                + level_bonus;
            (score > 0.0).then_some((score, report))
        })
        .collect::<Vec<_>>();
    scored_reports.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.1.level.cmp(&right.1.level))
    });
    let mut selected = scored_reports
        .into_iter()
        .take(8)
        .map(|(_, report)| report)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        selected = all_reports.iter().take(4).cloned().collect();
    }
    let selected_ids = selected
        .iter()
        .map(|report| report.id.clone())
        .collect::<HashSet<_>>();
    let parent_ids = selected
        .iter()
        .filter_map(|report| report.parent_id.clone())
        .filter(|id| !selected_ids.contains(id))
        .collect::<BTreeSet<_>>();
    for parent_id in parent_ids {
        if let Some(parent) = all_reports.iter().find(|report| report.id == parent_id) {
            selected.push(parent.clone());
        }
    }

    let mut context = String::new();
    let mut sources = Vec::new();
    let mut ids = Vec::new();

    if !hybrid.hits.is_empty() {
        context.push_str("[混合检索证据]\n");
        for hit in hybrid.hits.iter().take(10) {
            context.push_str(&format!(
                "- {}:{}｜{}\n",
                hit.source_kind, hit.source_id, hit.excerpt
            ));
            sources.push(hit.source_id.clone());
        }
        context.push('\n');
    }
    let mut entity_lines = Vec::new();
    for entity_id in entity_ids.iter().take(12) {
        let Ok(entity) = crate::entities::get_entity(conn, project_id, entity_id) else {
            continue;
        };
        entity_lines.push(format!(
            "- {}:{}｜{}｜{}",
            entity_id, entity.canonical_name, entity.entity_type, entity.description
        ));
        sources.push(entity_id.clone());
    }
    if !entity_lines.is_empty() {
        context.push_str("[实体链接与 PPR 子图]\n");
        context.push_str(&entity_lines.join("\n"));
        context.push_str("\n\n");
    }
    for report in &selected {
        context.push_str(&format!(
            "[社区 L{} {}：{}]\n{}\n来源：{}\n\n",
            report.level,
            report.title,
            report.id,
            report.content,
            report.source_ids.join(", ")
        ));
        ids.push(report.id.clone());
        sources.extend(report.source_ids.clone());
    }
    sources.sort();
    sources.dedup();
    Ok(InsightContext {
        context,
        source_ids: sources,
        community_ids: ids,
        entity_ids,
        retrieval_mode: hybrid.retrieval_mode,
        embedding_error: hybrid.embedding_error,
    })
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
        loaded
            .conn
            .execute(
                "INSERT INTO relation (id,project_id,from_id,to_id,type,status,created_at)
             VALUES ('r1',?1,'a','b','母子','confirmed','now'),
                    ('r2',?1,'b','c','夫妻','confirmed','now')",
                [&pid],
            )
            .unwrap();
        let reports = build_communities(&loaded.conn, &pid).unwrap();
        assert!(!reports.is_empty());
        assert!(reports.iter().all(|report| !report.source_ids.is_empty()));
        let graph = graph_data(&loaded.conn, &pid).unwrap();
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        let impact = impact(&loaded.conn, &pid, "a").unwrap();
        assert!(impact.items.iter().any(|item| item.kind == "relation"));
    }

    #[test]
    fn community_detection_builds_stable_micro_and_macro_levels() {
        let mut graph = UnGraph::<String, String>::new_undirected();
        let a = graph.add_node("a".into());
        let b = graph.add_node("b".into());
        let c = graph.add_node("c".into());
        let d = graph.add_node("d".into());
        let e = graph.add_node("e".into());
        let f = graph.add_node("f".into());
        for (left, right) in [(a, b), (b, c), (c, a), (d, e), (e, f), (f, d), (c, d)] {
            graph.add_edge(left, right, "relation".into());
        }
        let first = detect_community_hierarchy(&graph);
        let second = detect_community_hierarchy(&graph);
        assert_eq!(
            first
                .iter()
                .map(|group| group.key.clone())
                .collect::<Vec<_>>(),
            second
                .iter()
                .map(|group| group.key.clone())
                .collect::<Vec<_>>()
        );
        assert!(first.iter().any(|group| group.level == 0));
        let macro_groups = first
            .iter()
            .filter(|group| group.level > 0)
            .collect::<Vec<_>>();
        assert!(!macro_groups.is_empty());
        assert!(first
            .iter()
            .filter(|group| group.level == 0)
            .all(|group| group.parent_key.is_some()));
    }

    #[test]
    fn community_rebuild_keeps_ids_and_insight_uses_alias_ppr_subgraph() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&crate::domain::CreateProjectInput {
            name: "insight".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        loaded
            .conn
            .execute_batch(&format!(
                "INSERT INTO entity
                 (id,project_id,type,canonical_name,aliases,status,description,created_at,updated_at)
                 VALUES ('a','{pid}','character','林澈','[\"阿澈\"]','confirmed','寻找父亲死亡真相','now','now'),
                        ('b','{pid}','character','守夜人','[]','confirmed','隐瞒旧案','now','now'),
                        ('c','{pid}','item','旧钥匙','[]','confirmed','打开禁墟','now','now');
                 INSERT INTO relation (id,project_id,from_id,to_id,type,status,created_at)
                 VALUES ('ab','{pid}','a','b','敌对','confirmed','now'),
                        ('bc','{pid}','b','c','保管','confirmed','now');
                 INSERT INTO text_fts
                 (content,tokenized,source_id,project_id,source_kind,chapter_id,branch_id)
                 VALUES ('阿澈握着旧钥匙质问守夜人','阿澈 握着 旧钥匙 质问 守夜人','a','{pid}','entity',NULL,NULL);"
            ))
            .unwrap();
        let first = build_communities(&loaded.conn, &pid).unwrap();
        let first_ids = first
            .iter()
            .map(|report| (report.community_key.clone(), report.id.clone()))
            .collect::<BTreeMap<_, _>>();
        let second = build_communities(&loaded.conn, &pid).unwrap();
        for report in &second {
            assert_eq!(first_ids.get(&report.community_key), Some(&report.id));
        }
        let report = second.first().unwrap();
        let input = community_summary_input(&loaded.conn, &pid, &report.id).unwrap();
        let cited = validate_grounded_summary(
            &format!("关系张力持续存在 [{}]", input.source_ids[0]),
            &input.source_ids,
        )
        .unwrap();
        apply_llm_summary(
            &loaded.conn,
            &pid,
            &report.id,
            &report.source_hash,
            "关系张力持续存在",
            &cited,
        )
        .unwrap();
        let third = build_communities(&loaded.conn, &pid).unwrap();
        let preserved = third.iter().find(|item| item.id == report.id).unwrap();
        assert_eq!(preserved.summary_mode, "llm");
        assert!(preserved.content.contains("关系张力持续存在"));
        let insight = insight_context(
            &loaded.conn,
            &pid,
            "阿澈为什么不信任守夜人？",
            None,
            Some("向量服务不可用".into()),
        )
        .unwrap();
        assert!(insight.entity_ids.contains(&"a".to_string()));
        assert_eq!(insight.retrieval_mode, "fts_ppr");
        assert!(!insight.community_ids.is_empty());
        assert!(insight.context.contains("实体链接与 PPR 子图"));
        assert_eq!(insight.embedding_error.as_deref(), Some("向量服务不可用"));
    }

    #[test]
    fn grounded_summary_rejects_missing_or_unknown_citations() {
        let sources = vec!["claim-1".to_string(), "event-2".to_string()];
        assert_eq!(
            validate_grounded_summary("关系变化 [claim-1][event-2]", &sources).unwrap(),
            sources
        );
        assert!(validate_grounded_summary("没有引用", &sources).is_err());
        assert!(validate_grounded_summary("伪造来源 [claim-9]", &sources).is_err());
    }
}
