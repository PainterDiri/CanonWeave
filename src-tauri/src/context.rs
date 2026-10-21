use rusqlite::{params, Connection};

use crate::{
    domain::{ContextPreview, ContextSection, SceneRecord},
    error::AppResult,
    blueprint, retrieval, scenes, seed, style,
};

const LAYER_BUDGETS: &[(&str, &str, i32)] = &[
    ("L0", "HARD CANON", 12),
    ("L1", "ACTIVE STATE", 25),
    ("L2", "POV KNOWLEDGE", 6),
    ("L3", "WIKI INDEX", 4),
    ("L4", "DUE PROMISES", 9),
    ("L5", "EVIDENCE EXCERPTS", 25),
    ("L6", "VOICE & STYLE", 14),
    ("L7", "TASK CONTRACT", 5),
    ("L8", "FINAL HARD CONSTRAINTS", 10),
];

fn clip(text: &str, max: i32) -> String {
    let max = max.max(40) as usize;
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(max.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

fn claims_for_layer(
    conn: &Connection,
    project_id: &str,
    layer: &str,
    pov: &str,
    branch_id: Option<&str>,
) -> AppResult<String> {
    let sql = match layer {
        "L0" => {
            "SELECT subject, predicate, object, source_span, locked FROM claim
             WHERE project_id=?1 AND (branch_id IS NULL OR branch_id=?2)
               AND status='confirmed' AND truth_layer='world'
             ORDER BY locked DESC, updated_at DESC LIMIT 80"
        }
        _ => {
            "SELECT subject, predicate, object, source_span, locked FROM claim
             WHERE project_id=?1 AND (branch_id IS NULL OR branch_id=?2) AND status='confirmed'
               AND (truth_layer='world' OR perspective_id=?3 OR perspective_id IS NULL)
               AND truth_layer IN ('world','belief','utterance')
             ORDER BY truth_layer='world' DESC, updated_at DESC LIMIT 60"
        }
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = if layer == "L0" {
        stmt.query_map(params![project_id, branch_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i32>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(params![project_id, branch_id, pov], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i32>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    drop(stmt);
    if rows.is_empty() {
        return Ok(match layer {
            "L0" => "尚无已确认的世界事实。".into(),
            _ => "当前视角没有额外已确认认知。".into(),
        });
    }
    Ok(rows
        .into_iter()
        .map(|(subject, predicate, object, span, locked)| {
            let lock = if locked == 1 { " [锁定]" } else { "" };
            let evidence = if span.is_empty() {
                String::new()
            } else {
                format!("（{span}）")
            };
            format!("- {subject} · {predicate} · {object}{lock}{evidence}")
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

fn promises_block(conn: &Connection, project_id: &str) -> AppResult<String> {
    let mut stmt = conn.prepare(
        "SELECT title, state, target_window, payoff_contract FROM promise
         WHERE project_id=?1 AND state NOT IN ('paid_off','abandoned')
         ORDER BY CASE state WHEN 'ready' THEN 0 WHEN 'reinforced' THEN 1 ELSE 2 END LIMIT 20",
    )?;
    let rows = stmt
        .query_map([project_id], |row| {
            Ok(format!(
                "- [{}] {}｜窗口：{}｜兑现契约：{}",
                row.get::<_, String>(1)?,
                row.get::<_, String>(0)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if rows.is_empty() {
        Ok("当前没有未结伏笔。".into())
    } else {
        Ok(rows.join("\n"))
    }
}

fn evidence_block(
    conn: &Connection,
    project_id: &str,
    scene: &SceneRecord,
    due_titles: &str,
) -> AppResult<String> {
    let query = format!(
        "{} {} {} {}",
        scene.goal,
        scene.conflict,
        scene.pov,
        due_titles
    );
    let hits = retrieval::search_for_scene(conn, project_id, &query, 8, Some(&scene.id))?;
    let pov_id = retrieval::entity_id_by_name(conn, project_id, &scene.pov)?;
    let mut seeds = Vec::new();
    if let Some(id) = pov_id {
        seeds.push(id);
    }
    if let Some(place) = &scene.place_id {
        seeds.push(place.clone());
    }
    let graph = retrieval::ppr(conn, project_id, &seeds, 0.85)?;
    let mut ranked: Vec<(String, f64)> = graph.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let top_ids: Vec<String> = ranked.into_iter().take(4).map(|(id, _)| id).collect();
    let graph_claims = retrieval::graph_claims(conn, project_id, &top_ids, 4)?;
    let mut blocks = Vec::new();
    if !hits.is_empty() {
        blocks.push("[FTS5 原文命中]".to_string());
        for hit in hits {
            blocks.push(format!(
                "- {}（来源 {}，相关度 {}）",
                hit.excerpt, hit.source_id, hit.score
            ));
        }
    }
    if !graph_claims.is_empty() {
        blocks.push("[图谱多跳事实]".to_string());
        for (id, line, span) in graph_claims {
            blocks.push(format!("- {line}（{id}｜{span}）"));
        }
    }
    if blocks.is_empty() {
        Ok("尚无可用原文证据。写作时不得把未知设定当作已确认事实。".into())
    } else {
        Ok(blocks.join("\n"))
    }
}

fn voice_block(
    conn: &Connection,
    project_id: &str,
    scene: &SceneRecord,
    style_override: Option<&str>,
) -> AppResult<String> {
    let mut parts = Vec::new();
    if let Some(entity_id) = retrieval::entity_id_by_name(conn, project_id, &scene.pov)? {
        let applied_samples = crate::voice::context_anchor(conn, project_id, &entity_id, 1200)?;
        if !applied_samples.trim().is_empty() {
            parts.push(format!("[POV 人物声音样本与可解释指标]\n{applied_samples}"));
        }
        if let Ok(anchor) = conn.query_row(
            "SELECT voice_anchor, desire, fear FROM character_core WHERE entity_id=?1",
            [&entity_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        ) {
            if !anchor.0.trim().is_empty() && applied_samples.trim().is_empty() {
                parts.push(format!(
                    "[POV 声音锚点]\n{}",
                    clip(&anchor.0, 900)
                ));
            }
            if !anchor.1.is_empty() || !anchor.2.is_empty() {
                parts.push(format!(
                    "[POV 驱动力] 欲望：{}｜恐惧：{}",
                    anchor.1, anchor.2
                ));
            }
        }
    }
    let profile_id = style_override
        .map(str::to_string)
        .or_else(|| scene.style_profile_id.clone());
    if let Some(profile_id) = profile_id {
        let profile = style::get(conn, project_id, &profile_id)?;
        parts.push(style::contract(&profile));
    } else if let Some(profile) = style::list(conn, project_id)?
        .into_iter()
        .find(|profile| profile.scope == "project")
    {
        parts.push(style::contract(&profile));
    } else {
        parts.push("[STYLE CONTRACT] 尚未建立文笔档案：使用作者当前样本与场景契约，不添加套话。".into());
    }
    Ok(parts.join("\n\n"))
}

fn final_constraints(conn: &Connection, project_id: &str) -> AppResult<String> {
    let mut stmt = conn.prepare(
        "SELECT code, description, severity FROM rule
         WHERE project_id=?1 AND enabled=1 AND constraint_layer='hard'",
    )?;
    let rules = stmt
        .query_map([project_id], |row| {
            Ok(format!(
                "- [{}][{}] {}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(1)?
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut block = String::from(
        "- 不得改变 L0 已确认事实、死亡状态、时间顺序、地点与持有物。\n\
         - 不得让 POV 使用其尚未获得、未目睹且未被告知的信息。\n\
         - 文笔偏好不能覆盖硬事实；发现冲突时保留事实并调整表达。",
    );
    if !rules.is_empty() {
        block.push_str("\n");
        block.push_str(&rules.join("\n"));
    }
    Ok(block)
}

pub fn compile(
    conn: &Connection,
    project_id: &str,
    scene_id: &str,
    budget: i32,
    style_override: Option<&str>,
) -> AppResult<ContextPreview> {
    let budget = budget.clamp(2000, 60000);
    let scene = scenes::get_scene(conn, project_id, scene_id)?;
    let branch = crate::branches::current(conn, project_id)?;
    let chapter = scenes::get_chapter(conn, project_id, &scene.chapter_id)?;
    let premise = seed::active(conn, project_id)?;
    let mut sections = Vec::new();
    let promises = promises_block(conn, project_id)?;
    let due_titles = promises
        .lines()
        .take(3)
        .map(|line| line.trim_start_matches("- ").replace('[', "").replace(']', ""))
        .collect::<Vec<_>>()
        .join(" ");

    for (layer, title, percent) in LAYER_BUDGETS {
        let section_budget = (budget as f64 * *percent as f64 / 100.0).round() as i32;
        let content = match *layer {
            "L0" => claims_for_layer(conn, project_id, "L0", &scene.pov, branch.as_deref())?,
            "L1" => {
                let premise_text = premise
                    .as_ref()
                    .map(|p| {
                        format!(
                            "种子卡：{}｜核心冲突：{}｜基调：{}｜视角约定：{}",
                            p.logline, p.core_conflict, p.tone, p.pov
                        )
                    })
                    .unwrap_or_else(|| "种子卡尚未确认。".into());
                let blueprint_text =
                    blueprint::chapter_logic_context(conn, project_id, chapter.number)?;
                format!(
                    "{premise_text}\n\n本章叙事契约：\n{blueprint_text}\n\n当前章节：第 {} 章《{}》\n本场目标：{}\n本场冲突：{}",
                    chapter.number, chapter.title, scene.goal, scene.conflict
                )
            }
            "L2" => claims_for_layer(conn, project_id, "L2", &scene.pov, branch.as_deref())?,
            "L3" => crate::wiki::context_index(conn, project_id)?,
            "L4" => promises.clone(),
            "L5" => evidence_block(conn, project_id, &scene, &due_titles)?,
            "L6" => voice_block(conn, project_id, &scene, style_override)?,
            "L7" => scenes::scene_contract(conn, project_id, &scene)?,
            _ => final_constraints(conn, project_id)?,
        };
        let content = clip(&content, section_budget);
        sections.push(ContextSection {
            layer: (*layer).into(),
            title: (*title).into(),
            chars: content.chars().count() as i32,
            budget: section_budget,
            content,
        });
    }
    let hash_input = sections
        .iter()
        .map(|section| format!("[{}]{}", section.layer, section.content))
        .collect::<Vec<_>>()
        .join("\n");
    let total_chars = sections.iter().map(|section| section.chars).sum();
    Ok(ContextPreview {
        sections,
        total_chars,
        budget,
        context_hash: crate::hash_text(&hash_input),
    })
}

pub fn render(preview: &ContextPreview) -> String {
    preview
        .sections
        .iter()
        .map(|section| {
            format!(
                "[{} {}]\n{}",
                section.layer, section.title, section.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use crate::{
        domain::{CreateProjectInput, SceneInput, SensoryPlan, EmotionBeat},
        project, scenes,
    };

    use super::*;

    #[test]
    fn compiles_all_layers_with_evidence_and_stable_hash() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&CreateProjectInput {
            name: "t".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        let chapter_id = uuid::Uuid::new_v4().to_string();
        loaded
            .conn
            .execute(
                "INSERT INTO chapter (id, project_id, number, title, status, style_epoch, created_at, updated_at)
                 VALUES (?1,?2,1,'雨夜','planned',1,'now','now')",
                params![chapter_id, pid],
            )
            .unwrap();
        loaded
            .conn
            .execute(
                "INSERT INTO claim (id,project_id,subject,predicate,object,truth_layer,source_span,status,version,created_at,updated_at)
                 VALUES ('c1',?1,'父亲','已死亡','','world','ch03:p42','confirmed',1,'now','now')",
                [&pid],
            )
            .unwrap();
        let scene = scenes::save_scene(
            &loaded.conn,
            &pid,
            &SceneInput {
                id: None,
                chapter_id,
                goal: "找到父亲的信".into(),
                conflict: "母亲阻止".into(),
                pov: "林澈".into(),
                story_time: "葬礼夜".into(),
                place_id: None,
                style_profile_id: None,
                emotion_beat: EmotionBeat {
                    target: "压抑".into(),
                    intensity: 3,
                    entry: String::new(),
                    exit: String::new(),
                    turn: String::new(),
                },
                sensory_plan: SensoryPlan {
                    channels: vec!["触觉".into()],
                    motifs: vec!["信".into()],
                    distance: "近景".into(),
                },
            },
        )
        .unwrap();
        let preview = compile(&loaded.conn, &pid, &scene.id, 10000, None).unwrap();
        assert_eq!(preview.sections.len(), 9);
        assert!(preview
            .sections
            .iter()
            .any(|section| section.content.contains("父亲")));
        assert!(preview.total_chars <= preview.budget);
        let again = compile(&loaded.conn, &pid, &scene.id, 10000, None).unwrap();
        assert_eq!(preview.context_hash, again.context_hash);
    }
}
