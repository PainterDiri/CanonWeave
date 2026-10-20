use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use crate::{
    domain::{ImportResult, SourceDocument},
    error::{AppError, AppResult},
    now,
};

pub fn import_document(
    root: &Path,
    conn: &mut Connection,
    project_id: &str,
    path: &Path,
) -> AppResult<ImportResult> {
    if !path.is_file() {
        return Err(AppError::Message("导入文件不存在".into()));
    }
    let ext = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bytes = std::fs::read(path)?;
    let text = match ext.as_str() {
        "txt" => decode_text(&bytes),
        "md" | "markdown" => decode_text(&bytes),
        "docx" => extract_docx(&bytes)?,
        _ => {
            return Err(AppError::Message(
                "只支持 TXT、Markdown 与 DOCX 导入".into(),
            ))
        }
    };
    if text.trim().is_empty() {
        return Err(AppError::Message("导入文件没有可读取的正文".into()));
    }

    let hash = hex::encode(Sha256::digest(&bytes));
    let file_name = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("import.txt");
    let imports_dir = root.join("imports");
    std::fs::create_dir_all(&imports_dir)?;
    let stored = imports_dir.join(format!("{}-{}", &hash[..10], file_name));
    if !stored.exists() {
        std::fs::copy(path, &stored)?;
    }
    let title = path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("导入稿件")
        .to_string();
    let document_id = uuid::Uuid::new_v4().to_string();
    let ts = now();
    let chapters = split_chapters(&text, &title);
    let word_count = text.chars().filter(|c| !c.is_whitespace()).count() as i32;

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO source_document (id, project_id, path, hash, type, title, word_count, imported_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            document_id,
            project_id,
            stored.to_string_lossy(),
            hash,
            ext,
            title,
            word_count,
            ts
        ],
    )?;
    let mut chapters_created = 0;
    for (index, (chapter_title, chapter_text)) in chapters.iter().enumerate() {
        let number = (index + 1) as i32;
        let chapter_id = uuid::Uuid::new_v4().to_string();
        let existing: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM chapter WHERE project_id=?1 AND number=?2 AND status='imported')",
            params![project_id, number],
            |row| row.get(0),
        )?;
        if !existing {
            tx.execute(
                "INSERT INTO chapter (id, project_id, number, title, status, style_epoch, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,'imported',1,?5,?5)",
                params![chapter_id, project_id, number, chapter_title, ts],
            )?;
            chapters_created += 1;
        }
        let tokenized = crate::retrieval::tokenize(chapter_text);
        tx.execute(
            "INSERT INTO text_fts (content, tokenized, source_id, project_id, source_kind, chapter_id, branch_id)
             VALUES (?1,?2,?3,?4,'chapter',?5,NULL)",
            params![
                chapter_text,
                tokenized,
                format!("{document_id}:{number}"),
                project_id,
                chapter_id
            ],
        )?;
    }
    tx.commit()?;

    Ok(ImportResult {
        document: SourceDocument {
            id: document_id,
            path: stored.to_string_lossy().into_owned(),
            hash,
            doc_type: ext,
            title,
            word_count,
            imported_at: ts,
        },
        chapters_created,
    })
}

pub fn decode_text(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    let (decoded, _, _) = encoding_rs::GB18030.decode(bytes);
    decoded.into_owned()
}

pub fn extract_docx(bytes: &[u8]) -> AppResult<String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| AppError::Message(format!("DOCX 不是有效的压缩包：{e}")))?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|e| AppError::Message(format!("DOCX 缺少 word/document.xml：{e}")))?
        .read_to_string(&mut xml)?;

    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(false);
    let mut output = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = event.name();
                let local = name.local_name();
                if local.as_ref() == "p" {
                    if !output.ends_with('\n') && !output.is_empty() {
                        output.push('\n');
                    }
                } else if local.as_ref() == "tab" {
                    output.push('\t');
                } else if local.as_ref() == "br" {
                    output.push('\n');
                }
            }
            Ok(Event::Text(text)) => {
                output.push_str(text.as_ref());
            }
            Ok(Event::End(event)) => {
                if event.name().local_name().as_ref() == "p" {
                    output.push('\n');
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(AppError::Message(format!("解析 DOCX 失败：{e}"))),
            _ => {}
        }
    }
    Ok(output.trim().to_string())
}

pub fn split_chapters(text: &str, fallback_title: &str) -> Vec<(String, String)> {
    let pattern = Regex::new(
        r"(?m)^\s*(第[零一二三四五六七八九十百千万两\d]+[章节回卷][^\n\r]{0,40})\s*$",
    )
    .expect("chapter regex");
    let matches: Vec<_> = pattern.find_iter(text).collect();
    if matches.is_empty() {
        return vec![(fallback_title.to_string(), text.trim().to_string())];
    }
    let mut result = Vec::new();
    let prefix = text[..matches[0].start()].trim();
    for (index, item) in matches.iter().enumerate() {
        let end = matches.get(index + 1).map(|m| m.start()).unwrap_or(text.len());
        let body = text[item.end()..end].trim();
        let title = item.as_str().trim().to_string();
        if !body.is_empty() {
            result.push((title, body.to_string()));
        }
    }
    if !prefix.is_empty() {
        if let Some(first) = result.first_mut() {
            first.1 = format!("{prefix}\n\n{}", first.1);
        }
    }
    result
}

pub fn document(conn: &Connection, project_id: &str, id: &str) -> AppResult<SourceDocument> {
    Ok(conn.query_row(
        "SELECT id, path, hash, type, title, word_count, imported_at FROM source_document
         WHERE project_id=?1 AND id=?2",
        params![project_id, id],
        |row| {
            Ok(SourceDocument {
                id: row.get(0)?,
                path: row.get(1)?,
                hash: row.get(2)?,
                doc_type: row.get(3)?,
                title: row.get(4)?,
                word_count: row.get(5)?,
                imported_at: row.get(6)?,
            })
        },
    )?)
}

pub fn chapter_texts(
    conn: &Connection,
    project_id: &str,
    document_id: &str,
) -> AppResult<Vec<(i32, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT CAST(substr(source_id, instr(source_id, ':') + 1) AS INTEGER), chapter_id, content
         FROM text_fts WHERE project_id=?1 AND source_id LIKE ?2 ORDER BY chapter_id",
    )?;
    let rows = stmt
        .query_map(params![project_id, format!("{document_id}:%")], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<(i32, String, String)>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_chinese_chapters() {
        let text = "第一章 雨夜\n雨落在旧灯上。\n\n第二章 钥匙\n林澈握紧了钥匙。";
        let chapters = split_chapters(text, "稿");
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].0, "第一章 雨夜");
        assert!(chapters[0].1.contains("旧灯"));
        assert!(chapters[1].1.contains("钥匙"));
    }

    #[test]
    fn falls_back_to_single_chapter() {
        let chapters = split_chapters("只有一段正文。", "短篇");
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].0, "短篇");
    }
}
