//! Host-owned Instinct-style markdown memory.
//!
//! Durable files live in the workspace (`MEMORY.md`, `memory/YYYY-MM-DD.md`,
//! `.tasks/TODO.md`). Retrieval is keyword grep; the host injects a timestamped
//! one-pager into the system prompt. rx4 still owns the agent loop.

use std::path::{Path, PathBuf};

const MAX_PAGER_CHARS: usize = 8_000;
const MAX_SEARCH_HITS: usize = 8;
const MAX_SNIPPET_CHARS: usize = 160;

pub fn memory_md(workspace: &Path) -> PathBuf {
    workspace.join("MEMORY.md")
}

pub fn tasks_dir(workspace: &Path) -> PathBuf {
    workspace.join(".tasks")
}

pub fn todo_path(workspace: &Path) -> PathBuf {
    tasks_dir(workspace).join("TODO.md")
}

pub fn daily_note_path(workspace: &Path, day: chrono::NaiveDate) -> PathBuf {
    workspace
        .join("memory")
        .join(format!("{}.md", day.format("%Y-%m-%d")))
}

pub fn profile_pager(workspace: &Path) -> Option<String> {
    let path = memory_md(workspace);
    let content = std::fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    let body = truncate_chars(trimmed, MAX_PAGER_CHARS);
    let generated_at = chrono::Utc::now().to_rfc3339();
    Some(format!(
        "<memory_one_pager>\nGenerated at {generated_at}. Derived from MEMORY.md.\n\n{body}\n</memory_one_pager>"
    ))
}

pub fn inject_pager(base: &str, workspace: &Path) -> String {
    match profile_pager(workspace) {
        Some(pager) => format!("{base}\n\n{pager}"),
        None => base.to_string(),
    }
}

pub struct SearchHit {
    pub path: String,
    pub line_number: usize,
    pub snippet: String,
}

pub fn search_memory(workspace: &Path, query: &str) -> Vec<SearchHit> {
    let query_lower = query.to_lowercase();
    let words: Vec<&str> = query_lower
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() {
        return Vec::new();
    }

    let mut files = Vec::new();
    let memory = memory_md(workspace);
    if memory.is_file() {
        files.push(memory);
    }
    let todo = todo_path(workspace);
    if todo.is_file() {
        files.push(todo);
    }
    let dir = workspace.join("memory");
    if dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    files.push(path);
                }
            }
        }
    }

    let mut hits = Vec::new();
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            if words.iter().any(|word| lower.contains(word)) {
                hits.push(SearchHit {
                    path: path
                        .strip_prefix(workspace)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                    line_number: index + 1,
                    snippet: truncate_chars(line.trim(), MAX_SNIPPET_CHARS),
                });
                if hits.len() >= MAX_SEARCH_HITS {
                    return hits;
                }
            }
        }
    }
    hits
}

pub fn format_search(hits: &[SearchHit]) -> String {
    if hits.is_empty() {
        return "No markdown memory matched.".to_string();
    }
    let mut out = String::from("Memory search:\n");
    for hit in hits {
        out.push_str(&format!(
            "- {}:{} {}\n",
            hit.path, hit.line_number, hit.snippet
        ));
    }
    out
}

pub fn read_todo(workspace: &Path) -> String {
    match std::fs::read_to_string(todo_path(workspace)) {
        Ok(text) if !text.trim().is_empty() => text,
        _ => "No .tasks/TODO.md yet. `/todo <item>` appends a task.".to_string(),
    }
}

pub fn append_todo(workspace: &Path, item: &str) -> Result<String, String> {
    let trimmed = item.trim();
    if trimmed.is_empty() {
        return Err("Usage: /todo <item>".to_string());
    }
    let dir = tasks_dir(workspace);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = todo_path(workspace);
    let line = format!("- [ ] {trimmed}\n");
    if path.exists() {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|error| error.to_string())?;
        file.write_all(line.as_bytes())
            .map_err(|error| error.to_string())?;
    } else {
        std::fs::write(&path, format!("# TODO\n\n{line}")).map_err(|error| error.to_string())?;
    }
    Ok(format!("Added to .tasks/TODO.md: {trimmed}"))
}

pub fn append_session_note(workspace: &Path, line: &str) -> std::io::Result<()> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let dir = workspace.join("memory");
    std::fs::create_dir_all(&dir)?;
    let path = daily_note_path(workspace, chrono::Utc::now().date_naive());
    let stamp = chrono::Utc::now().format("%H:%M");
    let entry = format!("\n- [{stamp} UTC] {}\n", truncate_chars(trimmed, 200));
    if path.exists() {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&path)?;
        file.write_all(entry.as_bytes())?;
    } else {
        std::fs::write(
            path,
            format!(
                "# Session notes {}\n{entry}",
                chrono::Utc::now().format("%Y-%m-%d")
            ),
        )?;
    }
    Ok(())
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        text.to_string()
    } else {
        let head: String = text.chars().take(max_chars).collect();
        format!("{head}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pager_includes_generation_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("MEMORY.md"), "Prefers short plans.").unwrap();
        let pager = profile_pager(dir.path()).unwrap();
        assert!(pager.contains("Generated at"));
        assert!(pager.contains("Prefers short plans."));
        assert!(pager.contains("<memory_one_pager>"));
    }

    #[test]
    fn search_hits_memory_and_daily_notes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("MEMORY.md"), "Loves pasta.").unwrap();
        std::fs::create_dir_all(dir.path().join("memory")).unwrap();
        std::fs::write(
            dir.path().join("memory").join("2026-09-21.md"),
            "- takeout for lunch",
        )
        .unwrap();
        let hits = search_memory(dir.path(), "takeout");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("takeout"));
    }

    #[test]
    fn todo_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let added = append_todo(dir.path(), "ship memory").unwrap();
        assert!(added.contains("ship memory"));
        let listed = read_todo(dir.path());
        assert!(listed.contains("ship memory"));
    }
}
