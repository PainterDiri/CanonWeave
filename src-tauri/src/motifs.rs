use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::{
    domain::{MotifCheck, MotifHint, MotifRecord, SensoryPlan},
    error::AppResult,
};

fn list_json(value: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(value).unwrap_or_default()
}

pub fn rebuild(conn: &Connection, project_id: &str) -> AppResult<Vec<MotifRecord>> {
    let existing: HashMap<String, String> = {
        let mut stmt = conn.prepare(
            "SELECT name, meaning FROM motif WHERE project_id=?1 AND status='active'",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<HashMap<_, _>, _>>()?;
        drop(stmt);
        rows
    };
    let scenes = {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.goal, s.conflict, s.sensory_plan, c.id, c.number
             FROM scene s JOIN chapter c ON c.id=s.chapter_id
             WHERE s.project_id=?1 ORDER BY c.number, s.number",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i32>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        rows
    };
    let style_motifs = {
        let mut stmt = conn.prepare(
            "SELECT imagery FROM style_profile WHERE project_id=?1 AND status='confirmed'",
        )?;
        let rows = stmt
            .query_map([project_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        rows
    };
    let mut channels: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut first_scene: BTreeMap<String, String> = BTreeMap::new();
    let mut last_scene: BTreeMap<String, String> = BTreeMap::new();
    let mut counts: BTreeMap<String, i32> = BTreeMap::new();
    let mut counted_chapters = BTreeSet::new();
    for (scene_id, goal, conflict, sensory, chapter_id, _number) in scenes {
        let plan: SensoryPlan = serde_json::from_str(&sensory).unwrap_or(SensoryPlan {
            channels: vec![],
            motifs: vec![],
            distance: String::new(),
        });
        let mut local_text = format!("{goal}\n{conflict}");
        if counted_chapters.insert(chapter_id.clone()) {
            local_text.push('\n');
            local_text.push_str(&crate::scenes::chapter_text(conn, project_id, &chapter_id)?);
        }
        for motif in plan.motifs {
            let entry = channels.entry(motif.clone()).or_default();
            entry.extend(plan.channels.iter().cloned());
            let count = local_text.matches(&motif).count() as i32;
            *counts.entry(motif.clone()).or_default() += count;
            if count > 0 {
                first_scene.entry(motif.clone()).or_insert(scene_id.clone());
                last_scene.insert(motif, scene_id.clone());
            }
        }
    }
    for imagery in style_motifs {
        let value: Value = serde_json::from_str(&imagery).unwrap_or(Value::Null);
        let motifs = value
            .get("motifs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let style_channels = value
            .get("sensoryChannels")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for motif in motifs {
            if let Some(name) = motif.as_str() {
                let entry = channels.entry(name.to_string()).or_default();
                for channel in &style_channels {
                    if let Some(channel) = channel.as_str() {
                        entry.insert(channel.to_string());
                    }
                }
                counts.entry(name.to_string()).or_default();
            }
        }
    }

    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM motif WHERE project_id=?1", [project_id])?;
    let mut records = Vec::new();
    for (name, channel_set) in channels {
        let id = uuid::Uuid::new_v4().to_string();
        let count = counts.get(&name).copied().unwrap_or(0);
        let first = first_scene.get(&name).cloned();
        let last = last_scene.get(&name).cloned();
        let meaning = existing.get(&name).cloned().unwrap_or_default();
        let channel_list = channel_set.into_iter().collect::<Vec<_>>();
        tx.execute(
            "INSERT INTO motif (id, project_id, name, meaning, channels, first_scene_id,
                last_scene_id, use_count, status, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'active',?9)",
            params![
                id,
                project_id,
                name,
                meaning,
                serde_json::to_string(&channel_list)?,
                first,
                last,
                count,
                crate::now()
            ],
        )?;
        records.push(MotifRecord {
            id,
            name,
            meaning,
            channels: channel_list,
            first_scene_id: first,
            last_scene_id: last,
            use_count: count,
        });
    }
    tx.commit()?;
    records.sort_by(|left, right| {
        right
            .use_count
            .cmp(&left.use_count)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(records)
}

pub fn list(conn: &Connection, project_id: &str) -> AppResult<Vec<MotifRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, meaning, channels, first_scene_id, last_scene_id, use_count
         FROM motif WHERE project_id=?1 AND status='active'
         ORDER BY use_count DESC, name",
    )?;
    let rows = stmt
        .query_map([project_id], |row| {
            let channels: String = row.get(3)?;
            Ok(MotifRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                meaning: row.get(2)?,
                channels: list_json(&channels),
                first_scene_id: row.get(4)?,
                last_scene_id: row.get(5)?,
                use_count: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    Ok(rows)
}

fn relevant_motifs(
    conn: &Connection,
    project_id: &str,
    scene_id: &str,
    all: &[MotifRecord],
) -> AppResult<Vec<MotifRecord>> {
    let sensory: Option<String> = conn
        .query_row(
            "SELECT sensory_plan FROM scene WHERE project_id=?1 AND id=?2",
            params![project_id, scene_id],
            |row| row.get(0),
        )
        .optional()?;
    let names = sensory
        .and_then(|raw| serde_json::from_str::<SensoryPlan>(&raw).ok())
        .map(|plan| plan.motifs)
        .unwrap_or_default();
    if names.is_empty() {
        return Ok(all.iter().take(8).cloned().collect());
    }
    Ok(all
        .iter()
        .filter(|motif| names.contains(&motif.name))
        .cloned()
        .collect())
}

pub fn check(
    conn: &Connection,
    project_id: &str,
    scene_id: &str,
    text: &str,
) -> AppResult<MotifCheck> {
    rebuild(conn, project_id)?;
    let all = list(conn, project_id)?;
    let motifs = relevant_motifs(conn, project_id, scene_id, &all)?;
    let chars = text.chars().filter(|ch| !ch.is_whitespace()).count() as i32;
    let mut hints = Vec::new();
    for motif in &motifs {
        let count = text.matches(&motif.name).count() as i32;
        let per_1000 = if chars == 0 {
            0
        } else {
            (count as f64 * 1000.0 / chars as f64).round() as i32
        };
        if count >= 4 && per_1000 >= 8 {
            hints.push(MotifHint {
                motif: motif.name.clone(),
                count,
                per_1000,
                severity: if per_1000 >= 16 { "major" } else { "minor" }.into(),
                message: format!(
                    "“{}”在本片段出现 {} 次（约 {}/千字）；意象可能过密，考虑留白或换成动作承接。",
                    motif.name, count, per_1000
                ),
            });
        }
    }
    if motifs.len() > 2 {
        hints.push(MotifHint {
            motif: "本场母题组合".into(),
            count: motifs.len() as i32,
            per_1000: 0,
            severity: "minor".into(),
            message: format!(
                "本场使用 {} 个母题，超过建议的 1 至 2 个；可保留最能承载情绪的两个。",
                motifs.len()
            ),
        });
    }
    Ok(MotifCheck {
        chars,
        motifs,
        hints,
    })
}

#[cfg(test)]
mod tests {
    use crate::{domain::CreateProjectInput, project};

    use super::*;

    #[test]
    fn detects_overdense_motif_with_explainable_count() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = project::create(&CreateProjectInput {
            name: "motif".into(),
            root_path: dir.path().join("story").to_string_lossy().into_owned(),
            mode: "idea".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        loaded
            .conn
            .execute_batch(&format!(
                "INSERT INTO chapter (id,project_id,number,title,status,style_epoch,created_at,updated_at)
                 VALUES ('ch1','{pid}',1,'雨','planned',1,'now','now');
                 INSERT INTO scene (id,project_id,chapter_id,number,goal,conflict,sensory_plan,status,created_at,updated_at)
                 VALUES ('sc1','{pid}','ch1',1,'雨中等待','母亲拒绝开门',
                   '{{\"channels\":[\"视觉\",\"触觉\"],\"motifs\":[\"雨\"],\"distance\":\"近景\"}}',
                   'planned','now','now');"
            ))
            .unwrap();
        let check = check(&loaded.conn, &pid, "sc1", "雨落下来。雨更大了。雨声压住脚步。雨顺着窗沿流下。")
            .unwrap();
        assert!(check.motifs.iter().any(|motif| motif.name == "雨"));
        assert!(check.hints.iter().any(|hint| hint.motif == "雨"));
    }
}
