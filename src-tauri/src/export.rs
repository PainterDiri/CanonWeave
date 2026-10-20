use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::{
    domain::{ExportInput, ExportResult, WikiPage},
    error::{AppError, AppResult},
    now,
};

struct ChapterExport {
    number: i32,
    title: String,
    text: String,
}

struct ExportBundle {
    project_name: String,
    logline: String,
    core_conflict: String,
    chapters: Vec<ChapterExport>,
    wiki: Vec<WikiPage>,
}

fn load_bundle(conn: &Connection, project_id: &str, include_wiki: bool) -> AppResult<ExportBundle> {
    let project_name: String = conn.query_row(
        "SELECT name FROM project WHERE id=?1",
        [project_id],
        |row| row.get(0),
    )?;
    let (logline, core_conflict): (String, String) = conn
        .query_row(
            "SELECT logline, core_conflict FROM premise
             WHERE project_id=?1 AND status='confirmed' ORDER BY updated_at DESC LIMIT 1",
            [project_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or_default();
    let chapter_rows = {
        let mut stmt = conn.prepare(
            "SELECT number, title FROM chapter WHERE project_id=?1 ORDER BY number",
        )?;
        let rows = stmt
            .query_map([project_id], |row| {
                Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        rows
    };
    let mut chapters = Vec::new();
    for (number, title) in chapter_rows {
        let chapter_id: String = conn.query_row(
            "SELECT id FROM chapter WHERE project_id=?1 AND number=?2 LIMIT 1",
            rusqlite::params![project_id, number],
            |row| row.get(0),
        )?;
        let text = crate::scenes::chapter_text(conn, project_id, &chapter_id)?;
        chapters.push(ChapterExport {
            number,
            title,
            text,
        });
    }
    let wiki = if include_wiki {
        crate::wiki::list(conn, project_id)?
    } else {
        Vec::new()
    };
    Ok(ExportBundle {
        project_name,
        logline,
        core_conflict,
        chapters,
        wiki,
    })
}

fn markdown(bundle: &ExportBundle, includes_wiki: bool) -> String {
    let mut output = format!(
        "# {}\n\n> CanonWeave 本地导出 · {}\n\n",
        bundle.project_name,
        now()
    );
    if !bundle.logline.is_empty() {
        output.push_str(&format!("**一句话故事：** {}\n\n", bundle.logline));
    }
    if !bundle.core_conflict.is_empty() {
        output.push_str(&format!("**核心冲突：** {}\n\n", bundle.core_conflict));
    }
    output.push_str("## 目录\n\n");
    if bundle.chapters.is_empty() {
        output.push_str("- 尚无正文章节\n\n");
    }
    for chapter in &bundle.chapters {
        output.push_str(&format!(
            "- 第 {} 章 · {}\n",
            chapter.number, chapter.title
        ));
    }
    output.push('\n');
    for chapter in &bundle.chapters {
        output.push_str(&format!(
            "## 第 {} 章 · {}\n\n{}\n\n",
            chapter.number,
            chapter.title,
            if chapter.text.trim().is_empty() {
                "（本章尚无已提交正文）"
            } else {
                chapter.text.trim()
            }
        ));
    }
    if includes_wiki {
        output.push_str("# 叙事设定附录\n\n");
        output.push_str(
            "> 以下为 Narrative Wiki 派生视图，不是权威事实源；请以正文与项目数据库中的确认事实为准。\n\n",
        );
        for page in &bundle.wiki {
            output.push_str(&format!(
                "## {} · {}\n\n{}\n\n来源：{}\n\n",
                kind_label(&page.kind),
                page.title,
                page.content,
                page.source_ids.join(", ")
            ));
        }
    }
    output
}

fn kind_label(kind: &str) -> &str {
    match kind {
        "character" => "人物",
        "place" => "地点",
        "world_rule" => "规则",
        "timeline" => "时间线",
        "promise" => "伏笔",
        "chapter" => "章节摘要",
        _ => "页面",
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn docx_paragraph(text: &str, style: Option<&str>) -> String {
    let style_xml = style
        .map(|style| format!(r#"<w:pPr><w:pStyle w:val="{}"/></w:pPr>"#, xml_escape(style)))
        .unwrap_or_default();
    let runs = if text.is_empty() {
        "<w:r><w:t xml:space=\"preserve\"></w:t></w:r>".to_string()
    } else {
        text.lines()
            .enumerate()
            .map(|(index, line)| {
                let break_xml = if index == 0 {
                    ""
                } else {
                    "<w:r><w:br/></w:r>"
                };
                format!(
                    "{break_xml}<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r>",
                    xml_escape(line)
                )
            })
            .collect::<String>()
    };
    format!("<w:p>{style_xml}{runs}</w:p>")
}

fn docx_document(bundle: &ExportBundle, includes_wiki: bool) -> String {
    let mut paragraphs = vec![
        docx_paragraph(&bundle.project_name, Some("Title")),
        docx_paragraph(
            &format!("CanonWeave 本地导出 · {}", now()),
            Some("Subtitle"),
        ),
    ];
    if !bundle.logline.is_empty() {
        paragraphs.push(docx_paragraph(
            &format!("一句话故事：{}", bundle.logline),
            None,
        ));
    }
    if !bundle.core_conflict.is_empty() {
        paragraphs.push(docx_paragraph(
            &format!("核心冲突：{}", bundle.core_conflict),
            None,
        ));
    }
    for chapter in &bundle.chapters {
        paragraphs.push(docx_paragraph(
            &format!("第 {} 章 · {}", chapter.number, chapter.title),
            Some("Heading1"),
        ));
        paragraphs.push(docx_paragraph(
            if chapter.text.trim().is_empty() {
                "（本章尚无已提交正文）"
            } else {
                chapter.text.trim()
            },
            None,
        ));
    }
    if includes_wiki {
        paragraphs.push(docx_paragraph("叙事设定附录", Some("Heading1")));
        paragraphs.push(docx_paragraph(
            "以下为 Narrative Wiki 派生视图，不是权威事实源。",
            None,
        ));
        for page in &bundle.wiki {
            paragraphs.push(docx_paragraph(
                &format!("{} · {}", kind_label(&page.kind), page.title),
                Some("Heading2"),
            ));
            paragraphs.push(docx_paragraph(&page.content, None));
            paragraphs.push(docx_paragraph(
                &format!("来源：{}", page.source_ids.join(", ")),
                None,
            ));
        }
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>{}
<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr>
</w:body></w:document>"#,
        paragraphs.join("")
    )
}

fn write_docx(path: &Path, bundle: &ExportBundle, includes_wiki: bool) -> AppResult<()> {
    let file = std::fs::File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;
    let relationships = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
    let document_relationships = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;
    let styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:sz w:val="24"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:rPr><w:b/><w:sz w:val="40"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:rPr><w:color w:val="666666"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:style>
</w:styles>"#;
    for (name, content) in [
        ("[Content_Types].xml", content_types),
        ("_rels/.rels", relationships),
        ("word/document.xml", ""),
        ("word/_rels/document.xml.rels", document_relationships),
        ("word/styles.xml", styles),
    ] {
        zip.start_file(name, options)
            .map_err(|error| AppError::Message(format!("创建 DOCX 条目失败：{error}")))?;
        if name == "word/document.xml" {
            zip.write_all(docx_document(bundle, includes_wiki).as_bytes())?;
        } else {
            zip.write_all(content.as_bytes())?;
        }
    }
    zip.finish()
        .map_err(|error| AppError::Message(format!("完成 DOCX 导出失败：{error}")))?;
    Ok(())
}

fn safe_name(name: &str) -> String {
    let mut result = String::new();
    for ch in name.chars().take(40) {
        if ch.is_alphanumeric() || matches!(ch, '-' | '_') {
            result.push(ch);
        } else if ch.is_whitespace() {
            result.push('-');
        }
    }
    if result.is_empty() {
        "canonweave-export".into()
    } else {
        result
    }
}

fn stamp() -> String {
    now().chars()
        .filter(|ch| ch.is_alphanumeric())
        .take(17)
        .collect()
}

pub fn export(
    conn: &Connection,
    root: &Path,
    project_id: &str,
    input: &ExportInput,
) -> AppResult<ExportResult> {
    if !["markdown", "docx"].contains(&input.format.as_str()) {
        return Err(AppError::Message("导出格式只支持 markdown 或 docx".into()));
    }
    let bundle = load_bundle(conn, project_id, input.include_wiki)?;
    let words = bundle
        .chapters
        .iter()
        .map(|chapter| {
            chapter
                .text
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .count() as i32
        })
        .sum::<i32>();
    let exports = root.join("exports");
    std::fs::create_dir_all(&exports)?;
    let extension = if input.format == "markdown" { "md" } else { "docx" };
    let path: PathBuf = exports.join(format!(
        "{}-{}.{}",
        safe_name(&bundle.project_name),
        stamp(),
        extension
    ));
    if input.format == "markdown" {
        std::fs::write(&path, markdown(&bundle, input.include_wiki))?;
    } else {
        write_docx(&path, &bundle, input.include_wiki)?;
    }
    Ok(ExportResult {
        path: path.to_string_lossy().into_owned(),
        format: input.format.clone(),
        chapters: bundle.chapters.len() as i32,
        words,
        includes_wiki: input.include_wiki,
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{
            CreateProjectInput, PreferenceInput, VoiceSampleInput, WikiEditInput,
        },
        project,
    };

    use super::*;

    #[test]
    fn markdown_and_docx_export_reopen_with_wiki_voice_preference_and_revisions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("story");
        let mut loaded = project::create(&CreateProjectInput {
            name: "重开验收".into(),
            root_path: root.to_string_lossy().into_owned(),
            mode: "both".into(),
            writing_language: None,
        })
        .unwrap();
        let pid = loaded.summary.id.clone();
        loaded
            .conn
            .execute(
                "INSERT INTO entity (id,project_id,type,canonical_name,aliases,status,description,created_at,updated_at)
                 VALUES ('lin',?1,'character','林澈','[]','confirmed','主角','now','now')",
                [&pid],
            )
            .unwrap();
        let chapter_id = uuid::Uuid::new_v4().to_string();
        loaded
            .conn
            .execute(
                "INSERT INTO chapter (id,project_id,number,title,status,style_epoch,created_at,updated_at)
                 VALUES (?1,?2,1,'雨夜','planned',1,?3,?3)",
                rusqlite::params![chapter_id, pid, now()],
            )
            .unwrap();
        crate::scenes::commit_text(
            &mut loaded.conn,
            &pid,
            &chapter_id,
            "雨落在旧灯上。林澈把信收进衣袋，没有回头。",
            "manual_save",
        )
        .unwrap();
        crate::voice::add_sample(
            &loaded.conn,
            &pid,
            &VoiceSampleInput {
                character_id: "lin".into(),
                text: "林澈说：“先别下结论。把信给我。”他把信压在灯下。".into(),
                source_span: "作者样本 1".into(),
                source_type: "author".into(),
            },
        )
        .unwrap();
        crate::preferences::save(
            &loaded.conn,
            &pid,
            &PreferenceInput {
                scene_id: None,
                left_run_id: "a".into(),
                right_run_id: "b".into(),
                chosen_run_id: "a".into(),
                reason: "短句".into(),
                left_text: Some("灯灭。他停。门开。".into()),
                right_text: Some("灯光在漫长沉默里缓慢熄灭。".into()),
            },
        )
        .unwrap();
        crate::wiki::rebuild(&mut loaded.conn, &pid).unwrap();
        let _proposal = crate::wiki::propose_edit(
            &loaded.conn,
            &pid,
            &WikiEditInput {
                page_id: crate::wiki::list(&loaded.conn, &pid)
                    .unwrap()
                    .into_iter()
                    .find(|page| page.kind == "character")
                    .unwrap()
                    .id,
                title: "林澈".into(),
                content: "作者确认的派生页".into(),
            },
        )
        .unwrap();

        let markdown_path = export(
            &loaded.conn,
            &root,
            &pid,
            &ExportInput {
                format: "markdown".into(),
                include_wiki: true,
            },
        )
        .unwrap();
        let markdown_text = std::fs::read_to_string(&markdown_path.path).unwrap();
        assert!(markdown_text.contains("重开验收"));
        assert!(markdown_text.contains("雨落在旧灯上"));
        assert!(markdown_text.contains("叙事设定附录"));

        let docx_path = export(
            &loaded.conn,
            &root,
            &pid,
            &ExportInput {
                format: "docx".into(),
                include_wiki: true,
            },
        )
        .unwrap();
        let docx_bytes = std::fs::read(&docx_path.path).unwrap();
        let docx_text = crate::import::extract_docx(&docx_bytes).unwrap();
        assert!(docx_text.contains("重开验收"));
        assert!(docx_text.contains("林澈把信收进衣袋"));

        let created_at = loaded.summary.created_at.clone();
        drop(loaded);
        let reopened = project::open(&root).unwrap();
        assert_eq!(reopened.summary.id, pid);
        assert_eq!(reopened.summary.created_at, created_at);
        assert_eq!(reopened.summary.schema_version, 5);
        assert_eq!(crate::voice::profile(&reopened.conn, &pid, "lin").unwrap().sample_count, 1);
        assert_eq!(crate::preferences::summary(&reopened.conn, &pid, None).unwrap().choices, 1);
        assert!(crate::wiki::list(&reopened.conn, &pid)
            .unwrap()
            .iter()
            .any(|page| page.kind == "chapter"));
        let revisions: i32 = reopened
            .conn
            .query_row(
                "SELECT COUNT(*) FROM revision WHERE project_id=?1 AND chapter_id=?2",
                rusqlite::params![pid, chapter_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(revisions, 1);
        assert!(Path::new(&markdown_path.path).exists());
        assert!(Path::new(&docx_path.path).exists());
    }
}
