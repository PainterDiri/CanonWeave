use rusqlite::Connection;

use crate::{
    blueprint,
    domain::{FoundationCheck, FoundationReport},
    error::AppResult,
    seed,
};

fn check(
    code: &str,
    label: &str,
    passed: bool,
    message: &str,
    evidence: Vec<String>,
) -> FoundationCheck {
    FoundationCheck {
        code: code.into(),
        label: label.into(),
        severity: if passed { "pass" } else { "error" }.into(),
        message: message.into(),
        evidence,
    }
}

pub fn evaluate(conn: &Connection, project_id: &str) -> AppResult<FoundationReport> {
    let premise = seed::active(conn, project_id)?;
    let blueprints = match blueprint::current(conn, project_id)? {
        Some(view) => Some(view),
        None => blueprint::candidate(conn, project_id)?,
    };

    let confirmed_characters: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entity WHERE project_id=?1 AND type='character' AND status='confirmed'",
        [project_id],
        |row| row.get(0),
    )?;
    let blueprint_characters = blueprints
        .as_ref()
        .map(|view| {
            view.elements
                .iter()
                .filter(|element| element.kind == "character" && !element.name.trim().is_empty())
                .count()
        })
        .unwrap_or(0);
    let core_cast_count = confirmed_characters.max(blueprint_characters as i64);

    let relation_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM relation WHERE project_id=?1 AND status='confirmed'",
        [project_id],
        |row| row.get(0),
    )?;
    let narrative_relation_count = blueprints
        .as_ref()
        .map(|view| {
            view.elements
                .iter()
                .flat_map(|element| {
                    element
                        .relations
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|relation| {
                            relation
                                .get("target")
                                .and_then(serde_json::Value::as_str)
                                .is_some_and(|value| !value.trim().is_empty())
                                && relation
                                    .get("tension")
                                    .and_then(serde_json::Value::as_str)
                                    .is_some_and(|value| !value.trim().is_empty())
                        })
                })
                .count()
        })
        .unwrap_or(0);

    let faction_count = blueprints
        .as_ref()
        .map(|view| {
            view.elements
                .iter()
                .filter(|element| element.kind == "faction")
                .count() as i64
        })
        .unwrap_or(0)
        + conn.query_row(
            "SELECT COUNT(*) FROM entity
             WHERE project_id=?1 AND status='confirmed'
               AND (type IN ('faction','organization') OR type LIKE 'knowledge:faction%')",
            [project_id],
            |row| row.get::<_, i64>(0),
        )?;

    let constrained_world_rules = blueprints
        .as_ref()
        .map(|view| {
            view.elements
                .iter()
                .filter(|element| {
                    element.kind == "world_rule"
                        && element
                            .constraints
                            .iter()
                            .any(|constraint| !constraint.trim().is_empty())
                })
                .count()
        })
        .unwrap_or(0);
    let premise_world_rule = premise
        .as_ref()
        .map(|value| !value.world_rule.trim().is_empty())
        .unwrap_or(false);

    let constrained_knowledge: i64 = conn.query_row(
        "SELECT COUNT(*) FROM knowledge_entry
         WHERE project_id=?1 AND status='confirmed'
           AND (LENGTH(TRIM(summary)) > 0 OR fields NOT IN ('{}','[]','null'))",
        [project_id],
        |row| row.get(0),
    )?;
    let ability_cost = constrained_world_rules > 0 || constrained_knowledge > 0;

    let narrative_timeline = blueprints
        .as_ref()
        .map(|view| view.timeline.len())
        .unwrap_or(0);
    let event_timeline: i64 = conn.query_row(
        "SELECT COUNT(*) FROM event WHERE project_id=?1 AND status='confirmed'
         AND TRIM(story_time) != ''",
        [project_id],
        |row| row.get(0),
    )?;
    let timeline_anchors = narrative_timeline as i64 + event_timeline;

    let core_question = premise
        .as_ref()
        .map(|value| {
            !value.dramatic_question.trim().is_empty() || !value.core_conflict.trim().is_empty()
        })
        .unwrap_or(false)
        || blueprints
            .as_ref()
            .is_some_and(|view| !view.blueprint.core_dramatic_question.trim().is_empty());

    let checks = vec![
        check(
            "foundation.core_cast",
            "核心人物与对手",
            core_cast_count >= 2,
            if core_cast_count >= 2 {
                "已具备至少两个可发生目标冲突的核心角色。"
            } else {
                "至少需要主角与核心对手两个已明确角色。"
            },
            vec![format!("确认人物实体与蓝本人物合计：{core_cast_count}")],
        ),
        check(
            "foundation.relationship_tension",
            "关系张力",
            relation_count + narrative_relation_count as i64 > 0,
            if relation_count + narrative_relation_count as i64 > 0 {
                "已记录带冲突或张力的角色关系。"
            } else {
                "至少需要一条写明目标冲突或关系张力的关系。"
            },
            vec![format!(
                "确认关系 {} 条，蓝本张力关系 {} 条",
                relation_count, narrative_relation_count
            )],
        ),
        check(
            "foundation.faction_or_power",
            "主要势力或权力结构",
            faction_count > 0,
            if faction_count > 0 {
                "已记录能够施加结构性阻力的势力或组织。"
            } else {
                "需要一个掌握资源、规则或行动能力的主要势力。"
            },
            vec![format!("已识别势力/组织：{faction_count}")],
        ),
        check(
            "foundation.ability_cost",
            "关键能力与代价",
            ability_cost,
            if ability_cost {
                "关键机制已有摘要、边界或使用代价。"
            } else {
                "关键能力或规则必须写明代价、限制或失效条件。"
            },
            vec![format!(
                "带约束世界规则 {} 条，已填知识条目 {} 条",
                constrained_world_rules, constrained_knowledge
            )],
        ),
        check(
            "foundation.world_constraint",
            "世界约束",
            constrained_world_rules > 0 || premise_world_rule,
            if constrained_world_rules > 0 || premise_world_rule {
                "已存在会持续限制选择的世界规则。"
            } else {
                "需要一条可验证、可制造代价的世界规则。"
            },
            vec![format!(
                "蓝本约束规则 {} 条，种子卡世界规则：{}",
                constrained_world_rules,
                if premise_world_rule { "有" } else { "无" }
            )],
        ),
        check(
            "foundation.timeline_anchors",
            "时间线锚点",
            timeline_anchors >= 3,
            if timeline_anchors >= 3 {
                "已有足以判断前因、结果与长期后果的时间锚点。"
            } else {
                "至少需要三个时间锚点，并写明前因、结果和长期后果。"
            },
            vec![format!("时间锚点合计：{timeline_anchors}")],
        ),
        check(
            "foundation.open_questions",
            "核心戏剧问题",
            core_question,
            if core_question {
                "已明确当前必须回答的核心戏剧问题。"
            } else {
                "请先明确驱动主逻辑链的核心戏剧问题。"
            },
            vec![if core_question {
                "种子卡或叙事蓝本已有核心问题".into()
            } else {
                "种子卡与叙事蓝本均未填写核心问题".into()
            }],
        ),
    ];

    let error_count = checks
        .iter()
        .filter(|check| check.severity == "error")
        .count() as i32;
    let warning_count = checks
        .iter()
        .filter(|check| check.severity == "warning")
        .count() as i32;
    Ok(FoundationReport {
        ready: error_count == 0,
        passed_count: checks
            .iter()
            .filter(|check| check.severity == "pass")
            .count() as i32,
        total_count: checks.len() as i32,
        error_count,
        warning_count,
        checks,
    })
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::domain::CreateProjectInput;
    use crate::project;

    use super::*;

    fn empty_project() -> (tempfile::TempDir, crate::project::LoadedProject) {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&CreateProjectInput {
            name: "foundation".into(),
            root_path: dir.path().join("p").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        (dir, loaded)
    }

    #[test]
    fn empty_project_is_blocked_by_foundation() {
        let (_dir, loaded) = empty_project();
        let report = evaluate(&loaded.conn, &loaded.summary.id).unwrap();
        assert!(!report.ready);
        assert!(report.error_count >= 3);
        assert!(report
            .checks
            .iter()
            .any(|check| check.code == "foundation.core_cast" && check.severity == "error"));
    }

    #[test]
    fn report_serializes_for_agent_steps() {
        let (_dir, loaded) = empty_project();
        let report = evaluate(&loaded.conn, &loaded.summary.id).unwrap();
        let value = serde_json::to_value(report).unwrap();
        assert!(value.get("checks").unwrap().is_array());
    }

    #[allow(dead_code)]
    fn migrated(conn: &Connection) {
        let _ = conn;
    }
}
