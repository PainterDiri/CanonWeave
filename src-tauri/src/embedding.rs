use std::collections::HashSet;
use std::time::Duration;

use reqwest::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::{
    domain::{EmbeddingStatus, ModelConfig},
    error::{AppError, AppResult},
    now,
};

const MAX_DOCUMENT_CHARS: usize = 4000;

#[derive(Debug, Clone)]
pub struct EmbeddingRuntime {
    pub mode: String,
    pub model: String,
    pub base_url: String,
    pub dimensions: Option<i32>,
    pub batch_size: usize,
}

impl EmbeddingRuntime {
    pub fn from_config(config: &ModelConfig) -> AppResult<Self> {
        if !["off", "local", "api"].contains(&config.embedding_mode.as_str()) {
            return Err(AppError::Message("未知的 Embedding 模式".into()));
        }
        if config.embedding_mode == "off" {
            return Err(AppError::Message("Embedding 当前已关闭".into()));
        }
        let model = config
            .embedding_model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::Message("请先填写 Embedding 模型名".into()))?;
        let base_url = config
            .embedding_base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| config.base_url.clone());
        Ok(Self {
            mode: config.embedding_mode.clone(),
            model: model.to_string(),
            base_url,
            dimensions: config.embedding_dimensions.filter(|value| *value > 0),
            batch_size: config.embedding_batch_size.clamp(1, 256) as usize,
        })
    }

    pub fn requires_api_key(&self) -> bool {
        self.mode == "api"
    }
}

#[derive(Debug, Clone)]
pub struct EmbeddingDocument {
    pub owner_kind: String,
    pub owner_id: String,
    pub content: String,
    pub content_hash: String,
    pub branch_id: Option<String>,
}

impl EmbeddingDocument {
    fn new(
        owner_kind: &str,
        owner_id: impl Into<String>,
        content: impl Into<String>,
        branch_id: Option<String>,
    ) -> Option<Self> {
        let content = content
            .into()
            .trim()
            .chars()
            .take(MAX_DOCUMENT_CHARS)
            .collect::<String>();
        if content.chars().count() < 8 {
            return None;
        }
        Some(Self {
            owner_kind: owner_kind.into(),
            owner_id: owner_id.into(),
            content_hash: crate::hash_text(&content),
            content,
            branch_id,
        })
    }
}

#[derive(Debug)]
pub struct IndexPlan {
    pub documents: Vec<EmbeddingDocument>,
    pub skipped: i32,
}

#[derive(Debug, Clone)]
pub struct VectorHit {
    pub owner_kind: String,
    pub owner_id: String,
    pub content: String,
    pub score: f32,
}

fn text_array(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(value_text)
            .filter(|item| !item.trim().is_empty())
            .collect::<Vec<_>>()
            .join("；"),
        Value::Object(items) => items
            .iter()
            .filter_map(|(key, value)| {
                let value = value_text(value);
                (!value.trim().is_empty()).then(|| format!("{key}：{value}"))
            })
            .collect::<Vec<_>>()
            .join("；"),
        _ => value.to_string(),
    }
}

pub fn configured(config: &ModelConfig) -> bool {
    config.embedding_mode != "off" && config.embedding_model.is_some()
}

pub fn embedding_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/embeddings") {
        base.to_string()
    } else {
        format!("{base}/embeddings")
    }
}

pub async fn embed_texts(
    runtime: &EmbeddingRuntime,
    api_key: Option<&str>,
    texts: &[String],
) -> AppResult<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(vec![]);
    }
    if runtime.requires_api_key() && api_key.is_none() {
        return Err(AppError::Message(
            "API Embedding 模式需要已保存的 API Key".into(),
        ));
    }
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| AppError::Message(format!("初始化 Embedding 客户端失败：{error}")))?;
    let mut body = json!({
        "model": runtime.model,
        "input": texts
    });
    if let Some(dimensions) = runtime.dimensions {
        body["dimensions"] = json!(dimensions);
    }
    let mut request = client.post(embedding_url(&runtime.base_url)).json(&body);
    if let Some(key) = api_key.filter(|key| !key.trim().is_empty()) {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|error| AppError::Message(format!("Embedding 请求失败：{error}")))?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .map_err(|error| AppError::Message(format!("读取 Embedding 响应失败：{error}")))?;
    if !status.is_success() {
        let detail = raw.chars().take(400).collect::<String>();
        return Err(AppError::Message(format!(
            "Embedding 服务返回 {status}：{detail}"
        )));
    }
    let value: Value = serde_json::from_str(&raw)
        .map_err(|error| AppError::Message(format!("Embedding 响应不是合法 JSON：{error}")))?;
    let mut indexed = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Message("Embedding 响应缺少 data 数组".into()))?
        .iter()
        .map(|item| {
            let index = item.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            let vector = item
                .get("embedding")
                .and_then(Value::as_array)
                .ok_or_else(|| AppError::Message("Embedding 项缺少向量".into()))?
                .iter()
                .map(|value| {
                    value
                        .as_f64()
                        .map(|value| value as f32)
                        .ok_or_else(|| AppError::Message("Embedding 向量包含非数字值".into()))
                })
                .collect::<AppResult<Vec<f32>>>()?;
            Ok((index, normalize(&vector)))
        })
        .collect::<AppResult<Vec<_>>>()?;
    indexed.sort_by_key(|(index, _)| *index);
    if indexed.len() != texts.len() {
        return Err(AppError::Message(format!(
            "Embedding 服务返回 {} 条向量，但请求了 {} 条",
            indexed.len(),
            texts.len()
        )));
    }
    let dimensions = indexed.first().map(|(_, vector)| vector.len()).unwrap_or(0);
    if dimensions == 0 {
        return Err(AppError::Message("Embedding 服务返回了空向量".into()));
    }
    if runtime
        .dimensions
        .is_some_and(|expected| expected as usize != dimensions)
    {
        return Err(AppError::Message(format!(
            "Embedding 维度不匹配：配置为 {}，服务返回 {dimensions}",
            runtime.dimensions.unwrap_or_default()
        )));
    }
    if indexed.iter().any(|(_, vector)| vector.len() != dimensions) {
        return Err(AppError::Message("Embedding 返回的向量维度不一致".into()));
    }
    Ok(indexed.into_iter().map(|(_, vector)| vector).collect())
}

pub async fn embed_query(
    runtime: &EmbeddingRuntime,
    api_key: Option<&str>,
    query: &str,
) -> AppResult<Vec<f32>> {
    embed_texts(runtime, api_key, &[query.to_string()])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Message("Embedding 没有返回查询向量".into()))
}

fn normalize(vector: &[f32]) -> Vec<f32> {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return vector.to_vec();
    }
    vector.iter().map(|value| value / norm).collect()
}

fn vector_blob(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn blob_vector(blob: &[u8], dimensions: usize) -> Option<Vec<f32>> {
    if blob.len() != dimensions * 4 {
        return None;
    }
    Some(
        blob.chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

pub fn collect_documents(conn: &Connection, project_id: &str) -> AppResult<Vec<EmbeddingDocument>> {
    let mut documents = Vec::new();

    {
        let mut stmt = conn.prepare(
            "SELECT e.id, e.type, e.canonical_name, e.aliases, e.description,
                c.desire, c.need, c.fear, c.flaw, c.\"values\", c.secret, c.taboo
             FROM entity e LEFT JOIN character_core c ON c.entity_id=e.id
             WHERE e.project_id=?1 AND e.status='confirmed'
             ORDER BY e.updated_at DESC",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            let aliases = text_array(&row.get::<_, String>(3)?);
            let values = text_array(
                &row.get::<_, Option<String>>(9)?
                    .unwrap_or_else(|| "[]".into()),
            );
            let taboo = text_array(
                &row.get::<_, Option<String>>(11)?
                    .unwrap_or_else(|| "[]".into()),
            );
            Ok(EmbeddingDocument::new(
                "entity",
                row.get::<_, String>(0)?,
                format!(
                    "实体类型：{}；名称：{}；别名：{}；摘要：{}；欲望：{}；需求：{}；恐惧：{}；缺陷：{}；价值观：{}；秘密：{}；底线：{}",
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    aliases.join("、"),
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(7)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(8)?.unwrap_or_default(),
                    values.join("、"),
                    row.get::<_, Option<String>>(10)?.unwrap_or_default(),
                    taboo.join("、"),
                ),
                None,
            ))
        })?;
        for row in rows {
            if let Some(document) = row? {
                documents.push(document);
            }
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT e.id, d.name, e.name, e.aliases, e.summary, e.fields
             FROM knowledge_entry e JOIN knowledge_domain d ON d.id=e.domain_id
             WHERE e.project_id=?1 AND e.status='confirmed'
             ORDER BY e.updated_at DESC",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            let aliases = text_array(&row.get::<_, String>(3)?);
            let fields: Value =
                serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_else(|_| json!({}));
            Ok(EmbeddingDocument::new(
                "knowledge",
                row.get::<_, String>(0)?,
                format!(
                    "知识域：{}；名称：{}；别名：{}；摘要：{}；字段：{}",
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    aliases.join("、"),
                    row.get::<_, String>(4)?,
                    value_text(&fields),
                ),
                None,
            ))
        })?;
        for row in rows {
            if let Some(document) = row? {
                documents.push(document);
            }
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT id, subject, predicate, object, truth_layer, source_span, branch_id
             FROM claim WHERE project_id=?1 AND status='confirmed'
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok(EmbeddingDocument::new(
                "claim",
                row.get::<_, String>(0)?,
                format!(
                    "事实：{} · {} · {}；真值层：{}；来源：{}",
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ),
                row.get::<_, Option<String>>(6)?,
            ))
        })?;
        for row in rows {
            if let Some(document) = row? {
                documents.push(document);
            }
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT id, title, participants, story_time, reveal_time, cause, outcome, source_span, branch_id
             FROM event WHERE project_id=?1 AND status='confirmed'
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            let participants = text_array(&row.get::<_, String>(2)?);
            Ok(EmbeddingDocument::new(
                "event",
                row.get::<_, String>(0)?,
                format!(
                    "事件：{}；参与者：{}；故事时间：{}；叙述时间：{}；前因：{}；结果：{}；来源：{}",
                    row.get::<_, String>(1)?,
                    participants.join("、"),
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ),
                row.get::<_, Option<String>>(8)?,
            ))
        })?;
        for row in rows {
            if let Some(document) = row? {
                documents.push(document);
            }
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT id, title, state, target_window, payoff_contract, plant_evidence_ids,
                payoff_evidence_ids, payoff_state_change FROM promise WHERE project_id=?1",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            let plant = text_array(&row.get::<_, String>(5)?);
            let payoff = text_array(&row.get::<_, String>(6)?);
            Ok(EmbeddingDocument::new(
                "promise",
                row.get::<_, String>(0)?,
                format!(
                    "伏笔：{}；状态：{}；目标窗口：{}；兑现契约：{}；埋设证据：{}；兑现证据：{}；状态变化：{}",
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    plant.join("、"),
                    payoff.join("、"),
                    row.get::<_, String>(7)?,
                ),
                None,
            ))
        })?;
        for row in rows {
            if let Some(document) = row? {
                documents.push(document);
            }
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, stale_at FROM graph_community WHERE project_id=?1",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            let stale = row.get::<_, Option<String>>(3)?;
            Ok(stale.is_none().then(|| {
                EmbeddingDocument::new(
                    "community",
                    row.get::<_, String>(0).unwrap_or_default(),
                    format!(
                        "社区摘要：{}；{}",
                        row.get::<_, String>(1).unwrap_or_default(),
                        row.get::<_, String>(2).unwrap_or_default()
                    ),
                    None,
                )
            }))
        })?;
        for row in rows {
            if let Some(document) = row?.flatten() {
                documents.push(document);
            }
        }
    }

    {
        let mut stmt =
            conn.prepare("SELECT id, title, content, stale_at FROM wiki_page WHERE project_id=?1")?;
        let rows = stmt.query_map([project_id], |row| {
            let stale = row.get::<_, Option<String>>(3)?;
            Ok(stale.is_none().then(|| {
                EmbeddingDocument::new(
                    "wiki",
                    row.get::<_, String>(0).unwrap_or_default(),
                    format!(
                        "百科页面：{}；{}",
                        row.get::<_, String>(1).unwrap_or_default(),
                        row.get::<_, String>(2).unwrap_or_default()
                    ),
                    None,
                )
            }))
        })?;
        for row in rows {
            if let Some(document) = row?.flatten() {
                documents.push(document);
            }
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT c.id, c.number, c.title,
                COALESCE(GROUP_CONCAT(s.goal, '；'), ''),
                COALESCE(GROUP_CONCAT(s.conflict, '；'), ''),
                COALESCE(GROUP_CONCAT(s.pov, '；'), '')
             FROM chapter c LEFT JOIN scene s ON s.chapter_id=c.id
             WHERE c.project_id=?1 GROUP BY c.id, c.number, c.title
             ORDER BY c.number",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        for row in rows {
            let (id, number, title, goals, conflicts, points_of_view) = row?;
            let text = crate::scenes::chapter_text(conn, project_id, &id).unwrap_or_default();
            let excerpt = text.chars().take(1400).collect::<String>();
            if let Some(document) = EmbeddingDocument::new(
                "chapter",
                id,
                format!(
                    "第 {number} 章：{title}；场景目标：{goals}；冲突：{conflicts}；视角：{points_of_view}；正文摘录：{excerpt}"
                ),
                None,
            ) {
                documents.push(document);
            }
        }
    }

    documents.sort_by(|left, right| {
        left.owner_kind
            .cmp(&right.owner_kind)
            .then_with(|| left.owner_id.cmp(&right.owner_id))
    });
    documents.dedup_by(|left, right| {
        left.owner_kind == right.owner_kind && left.owner_id == right.owner_id
    });
    Ok(documents)
}

pub fn prepare_index(
    conn: &Connection,
    project_id: &str,
    runtime: &EmbeddingRuntime,
    rebuild: bool,
) -> AppResult<IndexPlan> {
    let documents = collect_documents(conn, project_id)?;
    if rebuild {
        conn.execute(
            "UPDATE embedding SET status='stale', updated_at=?3
             WHERE project_id=?1 AND model=?2",
            params![project_id, runtime.model, now()],
        )?;
    }
    let mut selected = Vec::new();
    let mut skipped = 0;
    for document in documents {
        let existing = conn
            .query_row(
                "SELECT dim, content_hash, status FROM embedding
                 WHERE project_id=?1 AND owner_kind=?2 AND owner_id=?3 AND model=?4",
                params![
                    project_id,
                    document.owner_kind,
                    document.owner_id,
                    runtime.model
                ],
                |row| {
                    Ok((
                        row.get::<_, i32>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let matches_dimension = runtime
            .dimensions
            .is_none_or(|dimensions| existing.as_ref().is_some_and(|item| item.0 == dimensions));
        if !rebuild
            && existing.as_ref().is_some_and(|item| {
                item.1 == document.content_hash && item.2 == "ready" && matches_dimension
            })
        {
            skipped += 1;
            continue;
        }
        if existing.as_ref().is_some_and(|item| {
            item.1 != document.content_hash || item.2 != "ready" || !matches_dimension
        }) {
            conn.execute(
                "UPDATE embedding SET status='stale', content=?5, content_hash=?6,
                    branch_id=?7, updated_at=?8
                 WHERE project_id=?1 AND owner_kind=?2 AND owner_id=?3 AND model=?4",
                params![
                    project_id,
                    document.owner_kind,
                    document.owner_id,
                    runtime.model,
                    document.content,
                    document.content_hash,
                    document.branch_id,
                    now()
                ],
            )?;
        }
        selected.push(document);
    }
    Ok(IndexPlan {
        documents: selected,
        skipped,
    })
}

pub fn apply_vectors(
    conn: &mut Connection,
    project_id: &str,
    runtime: &EmbeddingRuntime,
    documents: &[EmbeddingDocument],
    vectors: &[Vec<f32>],
) -> AppResult<()> {
    if documents.len() != vectors.len() {
        return Err(AppError::Message("向量数量与文档数量不一致".into()));
    }
    let ts = now();
    let tx = conn.transaction()?;
    for (document, vector) in documents.iter().zip(vectors) {
        tx.execute(
            "INSERT INTO embedding
             (id, project_id, owner_kind, owner_id, model, dim, vector, content,
              content_hash, status, branch_id, updated_at, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'ready',?10,?11,?11)
             ON CONFLICT(project_id, owner_kind, owner_id, model) DO UPDATE SET
               dim=excluded.dim, vector=excluded.vector, content=excluded.content,
               content_hash=excluded.content_hash, status='ready',
               branch_id=excluded.branch_id, updated_at=excluded.updated_at",
            params![
                uuid::Uuid::new_v4().to_string(),
                project_id,
                document.owner_kind,
                document.owner_id,
                runtime.model,
                vector.len() as i32,
                vector_blob(vector),
                document.content,
                document.content_hash,
                document.branch_id,
                ts
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn mark_batch_error(
    conn: &Connection,
    project_id: &str,
    model: &str,
    documents: &[EmbeddingDocument],
) -> AppResult<()> {
    let ts = now();
    for document in documents {
        conn.execute(
            "INSERT INTO embedding
             (id, project_id, owner_kind, owner_id, model, dim, vector, content,
              content_hash, status, branch_id, updated_at, created_at)
             VALUES (?1,?2,?3,?4,?5,0,X'',?6,?7,'error',?8,?9,?9)
             ON CONFLICT(project_id, owner_kind, owner_id, model) DO UPDATE SET
               content=excluded.content, content_hash=excluded.content_hash,
               status='error', branch_id=excluded.branch_id, updated_at=excluded.updated_at",
            params![
                uuid::Uuid::new_v4().to_string(),
                project_id,
                document.owner_kind,
                document.owner_id,
                model,
                document.content,
                document.content_hash,
                document.branch_id,
                ts
            ],
        )?;
    }
    Ok(())
}

pub fn finalize_index(
    conn: &Connection,
    project_id: &str,
    runtime: &EmbeddingRuntime,
) -> AppResult<i32> {
    let documents = collect_documents(conn, project_id)?;
    let expected: HashSet<(String, String)> = documents
        .iter()
        .map(|document| (document.owner_kind.clone(), document.owner_id.clone()))
        .collect();
    let rows = {
        let mut stmt = conn.prepare(
            "SELECT owner_kind, owner_id FROM embedding
             WHERE project_id=?1 AND model=?2 AND status IN ('ready','stale','pending')",
        )?;
        let rows = stmt
            .query_map(params![project_id, runtime.model], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let mut stale = 0;
    for (owner_kind, owner_id) in rows {
        if !expected.contains(&(owner_kind.clone(), owner_id.clone())) {
            conn.execute(
                "UPDATE embedding SET status='stale', updated_at=?4
                 WHERE project_id=?1 AND owner_kind=?2 AND owner_id=?3 AND model=?5",
                params![project_id, owner_kind, owner_id, now(), runtime.model],
            )?;
            stale += 1;
        }
    }
    Ok(stale)
}

pub fn mark_all_stale(conn: &Connection, project_id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE embedding SET status='stale', updated_at=?2
         WHERE project_id=?1 AND status IN ('ready','pending')",
        params![project_id, now()],
    )?;
    Ok(())
}

pub fn delete_kind_embeddings(
    conn: &Connection,
    project_id: &str,
    owner_kind: &str,
) -> AppResult<()> {
    conn.execute(
        "DELETE FROM embedding WHERE project_id=?1 AND owner_kind=?2",
        params![project_id, owner_kind],
    )?;
    Ok(())
}

pub fn vector_search(
    conn: &Connection,
    project_id: &str,
    model: &str,
    dimensions: i32,
    query: &[f32],
    branch_id: Option<&str>,
    limit: usize,
) -> AppResult<Vec<VectorHit>> {
    if query.len() != dimensions as usize {
        return Err(AppError::Message("查询向量维度不匹配".into()));
    }
    let query = normalize(query);
    let raw = {
        let mut stmt = conn.prepare(
            "SELECT owner_kind, owner_id, content, vector FROM embedding
             WHERE project_id=?1 AND model=?2 AND dim=?3 AND status='ready'
               AND (?5 IS NULL OR branch_id IS NULL OR branch_id=?5)
             LIMIT 2000",
        )?;
        let rows = stmt
            .query_map(
                params![project_id, model, dimensions, dimensions, branch_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let mut hits = raw
        .into_iter()
        .filter_map(|(owner_kind, owner_id, content, blob)| {
            let vector = blob_vector(&blob, dimensions as usize)?;
            let score = query.iter().zip(vector).map(|(a, b)| a * b).sum::<f32>();
            Some(VectorHit {
                owner_kind,
                owner_id,
                content,
                score,
            })
        })
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);
    Ok(hits)
}

pub fn status(
    conn: &Connection,
    project_id: &str,
    config: &ModelConfig,
) -> AppResult<EmbeddingStatus> {
    let configured = configured(config);
    let model = config.embedding_model.clone();
    let dimensions = config.embedding_dimensions;
    let (ready, stale, pending, error, other_model) = if let Some(model) = model.as_deref() {
        conn.query_row(
            "SELECT
               SUM(CASE WHEN model=?2 AND status='ready' THEN 1 ELSE 0 END),
               SUM(CASE WHEN model=?2 AND status='stale' THEN 1 ELSE 0 END),
               SUM(CASE WHEN model=?2 AND status='pending' THEN 1 ELSE 0 END),
               SUM(CASE WHEN model=?2 AND status='error' THEN 1 ELSE 0 END),
               SUM(CASE WHEN model!=?2 THEN 1 ELSE 0 END)
             FROM embedding WHERE project_id=?1",
            params![project_id, model],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?.unwrap_or(0) as i32,
                    row.get::<_, Option<i64>>(1)?.unwrap_or(0) as i32,
                    row.get::<_, Option<i64>>(2)?.unwrap_or(0) as i32,
                    row.get::<_, Option<i64>>(3)?.unwrap_or(0) as i32,
                    row.get::<_, Option<i64>>(4)?.unwrap_or(0) as i32,
                ))
            },
        )?
    } else {
        (0, 0, 0, 0, 0)
    };
    Ok(EmbeddingStatus {
        configured,
        mode: config.embedding_mode.clone(),
        model,
        dimensions,
        batch_size: config.embedding_batch_size,
        auto_index: config.embedding_auto_index,
        ready,
        stale,
        pending,
        error,
        other_model,
    })
}

pub fn link_entities(
    conn: &Connection,
    project_id: &str,
    query: &str,
    limit: usize,
) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id, canonical_name, aliases FROM entity
         WHERE project_id=?1 AND status='confirmed' LIMIT 500",
    )?;
    let rows = stmt.query_map([project_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            text_array(&row.get::<_, String>(2)?),
        ))
    })?;
    let mut linked = Vec::new();
    for row in rows {
        let (id, canonical, aliases) = row?;
        if query.contains(&canonical)
            || aliases
                .iter()
                .any(|alias| !alias.trim().is_empty() && query.contains(alias.trim()))
        {
            linked.push(id);
            if linked.len() >= limit {
                break;
            }
        }
    }
    Ok(linked)
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::Path;

    use rusqlite::Connection;

    use crate::domain::CreateProjectInput;
    use crate::model::{ApiKeyCache, ModelStore};
    use crate::project;

    use super::*;

    fn fixture() -> (tempfile::TempDir, project::LoadedProject) {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&CreateProjectInput {
            name: "embedding".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "both".into(),
            writing_language: None,
        })
        .unwrap();
        (dir, loaded)
    }

    fn runtime() -> EmbeddingRuntime {
        EmbeddingRuntime {
            mode: "api".into(),
            model: "test-embed".into(),
            base_url: "https://example.com/v1".into(),
            dimensions: Some(2),
            batch_size: 2,
        }
    }

    #[test]
    fn normalizes_embeddings_and_builds_endpoint() {
        let vector = normalize(&[3.0, 4.0]);
        assert!((vector[0] - 0.6).abs() < 0.0001);
        assert!((vector[1] - 0.8).abs() < 0.0001);
        assert_eq!(
            embedding_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/embeddings"
        );
    }

    #[tokio::test]
    async fn parses_openai_compatible_embedding_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 8192];
            let _ = stream.read(&mut request);
            let body = r#"{"data":[{"index":1,"embedding":[0,1]},{"index":0,"embedding":[3,4]}],"model":"test-embed"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let runtime = EmbeddingRuntime {
            mode: "api".into(),
            model: "test-embed".into(),
            base_url: format!("http://{address}/v1"),
            dimensions: Some(2),
            batch_size: 2,
        };
        let vectors = embed_texts(
            &runtime,
            Some("test-key"),
            &["第一条".into(), "第二条".into()],
        )
        .await
        .unwrap();
        assert_eq!(vectors.len(), 2);
        assert!((vectors[0][0] - 0.6).abs() < 0.001);
        assert!((vectors[0][1] - 0.8).abs() < 0.001);
        assert_eq!(vectors[1], vec![0.0, 1.0]);
        server.join().unwrap();
    }

    #[test]
    fn index_plan_skips_fresh_content_and_rebuilds_changed_hash() {
        let (_dir, loaded) = fixture();
        let pid = loaded.summary.id.clone();
        loaded
            .conn
            .execute(
                "INSERT INTO entity
                 (id,project_id,type,canonical_name,aliases,status,description,created_at,updated_at)
                 VALUES ('e',?1,'character','林澈','[\"阿澈\"]','confirmed','寻找父亲死亡真相的纪录片剪辑师','now','now')",
                rusqlite::params![pid],
            )
            .unwrap();
        let documents = collect_documents(&loaded.conn, &pid).unwrap();
        assert_eq!(documents.len(), 1);
        loaded
            .conn
            .execute(
                "INSERT INTO embedding
                 (id,project_id,owner_kind,owner_id,model,dim,vector,content,content_hash,status,
                  branch_id,updated_at,created_at)
                 VALUES ('v',?3,'entity','e','test-embed',2,X'',?1,?2,'ready',NULL,'now','now')",
                rusqlite::params![documents[0].content, documents[0].content_hash, pid],
            )
            .unwrap();
        let plan = prepare_index(&loaded.conn, &pid, &runtime(), false).unwrap();
        assert!(plan.documents.is_empty());
        assert_eq!(plan.skipped, 1);
        loaded
            .conn
            .execute(
                "UPDATE entity SET description='已改变', updated_at='later'",
                [],
            )
            .unwrap();
        let changed = prepare_index(&loaded.conn, &pid, &runtime(), false).unwrap();
        assert_eq!(changed.documents.len(), 1);
        let status: String = loaded
            .conn
            .query_row("SELECT status FROM embedding WHERE id='v'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(status, "stale");
    }

    #[test]
    fn vector_search_only_uses_ready_matching_model_and_dimension() {
        let (_dir, loaded) = fixture();
        let pid = loaded.summary.id.clone();
        for (id, owner, model, status, hash) in [
            ("ready", "alias", "test-embed", "ready", "h1"),
            ("stale", "old", "test-embed", "stale", "h2"),
            ("other", "other", "other-model", "ready", "h3"),
        ] {
            loaded
                .conn
                .execute(
                    "INSERT INTO embedding
                     (id,project_id,owner_kind,owner_id,model,dim,vector,content,content_hash,status,
                      branch_id,updated_at,created_at)
                     VALUES (?1,?2,'entity',?3,?4,2,X'0000803F00000000',?5,?6,?7,NULL,'now','now')",
                    rusqlite::params![id, pid, owner, model, format!("内容-{owner}"), hash, status],
                )
                .unwrap();
        }
        let hits =
            vector_search(&loaded.conn, &pid, "test-embed", 2, &[1.0, 0.0], None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].owner_id, "alias");
    }

    #[test]
    fn vector_search_filters_other_branches_but_keeps_shared_content() {
        let (_dir, loaded) = fixture();
        let pid = loaded.summary.id.clone();
        for (id, owner, branch) in [
            ("shared", "shared-owner", None),
            ("current", "current-owner", Some("branch-b")),
            ("other", "other-owner", Some("branch-c")),
        ] {
            loaded
                .conn
                .execute(
                    "INSERT INTO embedding
                     (id,project_id,owner_kind,owner_id,model,dim,vector,content,content_hash,status,
                      branch_id,updated_at,created_at)
                     VALUES (?1,?2,'claim',?3,'branch-embed',2,X'0000803F00000000',?4,?5,'ready',?6,'now','now')",
                    params![
                        id,
                        pid,
                        owner,
                        format!("内容-{owner}"),
                        format!("hash-{owner}"),
                        branch
                    ],
                )
                .unwrap();
        }
        let hits = vector_search(
            &loaded.conn,
            &pid,
            "branch-embed",
            2,
            &[1.0, 0.0],
            Some("branch-b"),
            10,
        )
        .unwrap();
        let owners = hits
            .into_iter()
            .map(|hit| hit.owner_id)
            .collect::<HashSet<_>>();
        assert!(owners.contains("shared-owner"));
        assert!(owners.contains("current-owner"));
        assert!(!owners.contains("other-owner"));
    }

    #[test]
    fn entity_linking_matches_aliases() {
        let (_dir, loaded) = fixture();
        let pid = loaded.summary.id.clone();
        loaded
            .conn
            .execute(
                "INSERT INTO entity
                 (id,project_id,type,canonical_name,aliases,status,description,created_at,updated_at)
                 VALUES ('e',?1,'character','林澈','[\"阿澈\"]','confirmed','','now','now');",
                rusqlite::params![pid],
            )
            .unwrap();
        let ids = link_entities(&loaded.conn, &pid, "阿澈为什么隐瞒", 5).unwrap();
        assert_eq!(ids, vec!["e"]);
    }

    #[test]
    fn semantic_eval_covers_alias_motivation_relationship_promise_and_world_rule() {
        let (_dir, loaded) = fixture();
        let pid = loaded.summary.id.clone();
        let cases = [
            (
                "alias",
                "entity",
                "owner-alias",
                "阿澈，林澈的常用称谓",
                [1.0_f32, 0.0, 0.0, 0.0, 0.0],
                [1.0_f32, 0.0, 0.0, 0.0, 0.0],
            ),
            (
                "motivation",
                "entity",
                "owner-motivation",
                "欲望：证明父亲死亡并非意外",
                [0.0, 1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0, 0.0],
            ),
            (
                "relationship_change",
                "claim",
                "owner-relationship",
                "林澈与顾鸢从敌对变为有限信任",
                [0.0, 0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0, 0.0],
            ),
            (
                "promise",
                "promise",
                "owner-promise",
                "下周寄出的信必须揭示寄信者",
                [0.0, 0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0, 0.0],
            ),
            (
                "world_rule",
                "knowledge",
                "owner-world-rule",
                "未来信每提前出现一次就损失一天记忆",
                [0.0, 0.0, 0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0, 0.0, 1.0],
            ),
        ];
        for (_, kind, owner, content, stored, _) in &cases {
            loaded
                .conn
                .execute(
                    "INSERT INTO embedding
                     (id,project_id,owner_kind,owner_id,model,dim,vector,content,content_hash,status,
                      branch_id,updated_at,created_at)
                     VALUES (?1,?2,?3,?4,'eval-embed',5,?5,?6,?7,'ready',NULL,'now','now')",
                    params![
                        format!("vector-{owner}"),
                        pid,
                        kind,
                        owner,
                        vector_blob(stored),
                        content,
                        crate::hash_text(content)
                    ],
                )
                .unwrap();
        }
        for (case, _, expected_owner, _, _, query) in &cases {
            let hits = vector_search(&loaded.conn, &pid, "eval-embed", 5, query, None, 1).unwrap();
            assert_eq!(
                hits.first().map(|hit| hit.owner_id.as_str()),
                Some(*expected_owner),
                "evaluation case {case} failed"
            );
        }
    }

    #[allow(dead_code)]
    fn _migration_smoke() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut conn).unwrap();
    }

    #[tokio::test]
    #[ignore = "requires a configured local API key and an embedding-capable provider"]
    async fn real_provider_smoke() {
        let config_dir = std::env::var("CANONWEAVE_CONFIG_DIR")
            .expect("CANONWEAVE_CONFIG_DIR must point to the CanonWeave app config directory");
        let model_override = std::env::var("CANONWEAVE_EMBEDDING_MODEL")
            .expect("CANONWEAVE_EMBEDDING_MODEL must name the provider embedding model");
        let store = ModelStore::new(Path::new(&config_dir));
        let mut config = store.load();
        config.embedding_mode =
            std::env::var("CANONWEAVE_EMBEDDING_MODE").unwrap_or_else(|_| "api".into());
        config.embedding_model = Some(model_override);
        config.embedding_base_url = std::env::var("CANONWEAVE_EMBEDDING_BASE_URL")
            .ok()
            .filter(|value| !value.is_empty());
        config.embedding_dimensions = std::env::var("CANONWEAVE_EMBEDDING_DIMENSIONS")
            .ok()
            .and_then(|value| value.parse().ok());
        let runtime = EmbeddingRuntime::from_config(&config).unwrap();
        let key = ApiKeyCache::default().get().unwrap();
        let vectors = embed_texts(
            &runtime,
            key.as_deref(),
            &[
                "林澈在雨夜找到黄铜钥匙".to_string(),
                "守夜人隐瞒了车站规则".to_string(),
            ],
        )
        .await
        .unwrap();
        assert_eq!(vectors.len(), 2);
        assert_eq!(vectors[0].len(), vectors[1].len());
        assert!(vectors[0].len() >= 16);
    }
}
