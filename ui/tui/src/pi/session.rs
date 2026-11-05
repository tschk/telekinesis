//! Pi JSONL v3 session format — entry types, header, persistence.
//!
//! Compatible with pi_agent_rust session format:
//! - Location: ~/.pi/agent/sessions/--encoded-project-path--/
//! - Filename: YYYY-MM-DDTHH-MM-SS.sssZ_id.jsonl
//! - Format: JSON Lines (header + entries)

use chrono::{DateTime, Utc};
use fs2::FileExt;
use rx4::provider::{Message, Role};
use rx4::TodoState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const MAX_SESSION_BYTES: usize = 10 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct SessionCheckpoint {
    version: u32,
    entries: usize,
    bytes: u64,
    sha256: String,
}

fn checkpoint_path(path: &Path) -> PathBuf {
    path.with_extension("jsonl.integrity")
}

fn lock_session(path: &Path, exclusive: bool) -> std::io::Result<std::fs::File> {
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("jsonl.lock"))?;
    if exclusive {
        FileExt::lock_exclusive(&lock)?;
    } else {
        FileExt::lock_shared(&lock)?;
    }
    Ok(lock)
}

fn checksum(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn invalid_data(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

/// Custom-entry extension name for tool calls appended by the TUI.
pub const TOOL_CALL_EXTENSION: &str = "telekinesis.tool_call";
/// Custom-entry extension name for tool results appended by the TUI.
pub const TOOL_RESULT_EXTENSION: &str = "telekinesis.tool_result";
/// Custom-entry extension carrying the engine-owned todo list.
pub const TODOS_EXTENSION: &str = "telekinesis.todos";
/// Durable `session_info` key written before a persist rewrite so a truncated
/// tail is not lost.
pub const INTERRUPTED_INFO_KEY: &str = "interrupted";
/// Esc-cancel `session_info` key so a cancelled turn is not classified as a crash.
pub const CANCELLED_INFO_KEY: &str = "cancelled";

/// Session header — first line of the JSONL file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionHeader {
    pub version: u32,
    pub id: String,
    pub project: String,
    pub created: DateTime<Utc>,
    pub model: String,
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl PiSessionHeader {
    pub fn new(project: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            version: crate::pi::PI_SESSION_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            project: project.into(),
            created: Utc::now(),
            model: model.into(),
            provider: None,
            label: None,
        }
    }
}

/// Entry types in a pi session (pi_agent_rust SessionEntry).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PiEntryType {
    #[serde(rename = "message")]
    Message {
        role: Role,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
    },
    #[serde(rename = "model_change")]
    ModelChange { from: String, to: String },
    #[serde(rename = "thinking_level_change")]
    ThinkingLevelChange { level: String },
    #[serde(rename = "compaction")]
    Compaction { summary: String, cut_at: usize },
    #[serde(rename = "branch_summary")]
    BranchSummary { from_session: String, at_entry: u64 },
    #[serde(rename = "session_info")]
    SessionInfo { key: String, value: String },
    #[serde(rename = "label")]
    Label { text: String },
    #[serde(rename = "custom")]
    Custom {
        extension: String,
        payload: serde_json::Value,
    },
}

/// A single entry in the session log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiEntry {
    #[serde(flatten)]
    pub entry_type: PiEntryType,
    pub timestamp: DateTime<Utc>,
    pub id: u64,
    pub parent_id: Option<u64>,
}

/// Pi-format session — JSONL v3 with typed entries and tree structure.
pub struct PiSession {
    pub header: PiSessionHeader,
    pub entries: Vec<PiEntry>,
    next_id: u64,
    persisted_entries: usize,
    persisted_bytes: u64,
    persisted_hash: Option<String>,
    persisted_path: Option<PathBuf>,
    integrity_status: String,
    can_append: bool,
    unverified_tail: bool,
}

impl PiSession {
    pub fn new(project: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            header: PiSessionHeader::new(project, model),
            entries: Vec::new(),
            next_id: 1,
            persisted_entries: 0,
            persisted_bytes: 0,
            persisted_hash: None,
            persisted_path: None,
            integrity_status: "unverified-legacy".into(),
            can_append: true,
            unverified_tail: false,
        }
    }

    /// Integrity of the snapshot that was loaded, before any subsequent save.
    pub fn integrity_status(&self) -> &str {
        &self.integrity_status
    }

    /// Continue a recovered prefix in a separate session, preserving the source tail.
    pub fn prepare_resume(&mut self) -> bool {
        if self.unverified_tail {
            self.header.id = uuid::Uuid::new_v4().to_string();
            self.header.created = Utc::now();
            self.persisted_entries = 0;
            self.persisted_bytes = 0;
            self.persisted_hash = None;
            self.persisted_path = None;
            self.unverified_tail = false;
            return true;
        }
        false
    }

    pub fn append(&mut self, entry_type: PiEntryType) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let parent = self.entries.last().map(|e| e.id);
        self.entries.push(PiEntry {
            entry_type,
            timestamp: Utc::now(),
            id,
            parent_id: parent,
        });
        id
    }

    pub fn append_message(&mut self, role: Role, content: impl Into<String>) -> u64 {
        self.append(PiEntryType::Message {
            role,
            content: content.into(),
            tool_call_id: None,
        })
    }

    pub fn append_tool_result(
        &mut self,
        tool_call_id: impl Into<String>,
        content: impl Into<String>,
    ) -> u64 {
        self.append(PiEntryType::Message {
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
        })
    }

    pub fn append_model_change(&mut self, from: impl Into<String>, to: impl Into<String>) -> u64 {
        self.append(PiEntryType::ModelChange {
            from: from.into(),
            to: to.into(),
        })
    }

    pub fn append_compaction(&mut self, summary: impl Into<String>, cut_at: usize) -> u64 {
        self.append(PiEntryType::Compaction {
            summary: summary.into(),
            cut_at,
        })
    }

    pub fn append_label(&mut self, text: impl Into<String>) -> u64 {
        self.append(PiEntryType::Label { text: text.into() })
    }

    /// Fork the session from a specific entry (pi branching).
    pub fn fork(&self, from_entry: u64) -> Self {
        let mut forked = Self::new(self.header.project.clone(), self.header.model.clone());
        forked.header.id = uuid::Uuid::new_v4().to_string();
        forked.header.label = Some(format!("fork of {} at {}", self.header.id, from_entry));

        for entry in &self.entries {
            forked.entries.push(PiEntry {
                entry_type: clone_entry_type(&entry.entry_type),
                timestamp: entry.timestamp,
                id: entry.id,
                parent_id: entry.parent_id,
            });
            if entry.id == from_entry {
                break;
            }
        }
        forked.next_id = self.next_id;
        forked
    }

    /// Path this session persists to.
    pub fn jsonl_path(&self, dir: &Path) -> std::path::PathBuf {
        dir.join(format!(
            "{}_{}.jsonl",
            self.header.created.format("%Y-%m-%dT%H-%M-%S%.3fZ"),
            &self.header.id[..8]
        ))
    }

    /// Save JSONL entries by appending, then atomically publish their checkpoint.
    pub fn save_jsonl(&mut self, dir: &Path) -> std::io::Result<std::path::PathBuf> {
        use std::io::Write;

        if !self.can_append {
            return Err(invalid_data(
                "legacy session contains incomplete or unsupported records; refusing to append",
            ));
        }
        self.prepare_resume();
        std::fs::create_dir_all(dir)?;
        let path = self
            .persisted_path
            .clone()
            .unwrap_or_else(|| self.jsonl_path(dir));
        let _lock = lock_session(&path, true)?;
        let existing = if let Some(expected) = &self.persisted_hash {
            let content = std::fs::read(&path)?;
            if content.len() as u64 != self.persisted_bytes || checksum(&content) != *expected {
                return Err(invalid_data(
                    "session changed since it was loaded; refusing to overwrite it",
                ));
            }
            content
        } else {
            Vec::new()
        };
        if self.persisted_entries > self.entries.len() {
            return Err(invalid_data(
                "session entries were removed; refusing to overwrite the saved file",
            ));
        }
        let mut added = Vec::new();
        if self.persisted_hash.is_none() {
            serde_json::to_writer(&mut added, &self.header)?;
            added.push(b'\n');
        }
        for entry in &self.entries[self.persisted_entries..] {
            serde_json::to_writer(&mut added, entry)?;
            added.push(b'\n');
        }
        if existing.len().saturating_add(added.len()) > MAX_SESSION_BYTES {
            return Err(invalid_data("session exceeds the 10 MiB size limit"));
        }
        let mut content = existing;
        content.extend_from_slice(&added);
        let hash = checksum(&content);
        let checkpoint = SessionCheckpoint {
            version: 1,
            entries: self.entries.len(),
            bytes: content.len() as u64,
            sha256: hash.clone(),
        };
        let mut temporary = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(dir))?;
        serde_json::to_writer(&mut temporary, &checkpoint)?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;

        let new_file = self.persisted_hash.is_none();
        let mut file = if new_file {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?
        } else {
            std::fs::OpenOptions::new().append(true).open(&path)?
        };
        let result = (|| {
            file.write_all(&added)?;
            file.sync_all()?;
            temporary
                .persist(checkpoint_path(&path))
                .map_err(|error| error.error)?;
            Ok(())
        })();
        if let Err(error) = result {
            if new_file {
                drop(file);
                let _ = std::fs::remove_file(&path);
            } else if file
                .set_len(self.persisted_bytes)
                .and_then(|()| file.sync_all())
                .is_err()
            {
                return Err(invalid_data(format!("session save failed ({error}); rollback failed, do not resume without inspecting the file")));
            }
            return Err(error);
        }
        self.persisted_entries = self.entries.len();
        self.persisted_bytes = content.len() as u64;
        self.persisted_hash = Some(hash);
        self.persisted_path = Some(path.clone());
        #[cfg(unix)]
        std::fs::File::open(path.parent().unwrap_or(dir))?.sync_all()?;
        Ok(path)
    }

    /// Load a JSONL v3 session file.
    pub fn load_jsonl(path: &Path) -> std::io::Result<Self> {
        use std::io::Read;
        let _lock = lock_session(path, false)?;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take((MAX_SESSION_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_SESSION_BYTES {
            return Err(invalid_data("session exceeds the 10 MiB size limit"));
        }
        let checkpoint = match std::fs::read(checkpoint_path(path)) {
            Ok(bytes) => Some(
                serde_json::from_slice::<SessionCheckpoint>(&bytes)
                    .map_err(|e| invalid_data(format!("invalid session checkpoint: {e}")))?,
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let mut integrity_status = "unverified-legacy".to_owned();
        let mut unverified_tail = false;
        if let Some(checkpoint) = &checkpoint {
            if checkpoint.version != 1 {
                return Err(invalid_data("unsupported session checkpoint version"));
            }
            if (bytes.len() as u64) < checkpoint.bytes {
                let records = bytes
                    .iter()
                    .filter(|byte| **byte == b'\n')
                    .count()
                    .saturating_sub(1);
                return Err(invalid_data(format!(
                    "session truncated-at-{records} records (expected {} records, {} bytes)",
                    checkpoint.entries, checkpoint.bytes
                )));
            }
            let committed = checkpoint.bytes as usize;
            if checksum(&bytes[..committed]) != checkpoint.sha256 {
                return Err(invalid_data(
                    "session integrity check failed; committed prefix differs from checkpoint",
                ));
            }
            let tail = &bytes[committed..];
            if tail.is_empty() {
                integrity_status = "verified".into();
            } else {
                let complete = tail.split_inclusive(|byte| *byte == b'\n');
                let mut records = 0;
                let mut other = 0;
                let mut partial = false;
                for line in complete {
                    if !line.ends_with(b"\n") {
                        partial = true;
                    } else if serde_json::from_slice::<PiEntry>(line).is_ok() {
                        records += 1;
                    } else {
                        other += 1;
                    }
                }
                integrity_status = format!(
                    "verified to record {}; {records} complete unverified records, {other} unrecognized lines{}; original preserved for inspection at {}; continuation uses a new session",
                    checkpoint.entries,
                    if partial { ", partial trailing record" } else { "" },
                    path.display()
                );
                unverified_tail = true;
            }
            bytes.truncate(committed);
        }
        let hash = checksum(&bytes);
        let content = String::from_utf8(bytes).map_err(|e| invalid_data(e.to_string()))?;
        let mut lines = content.lines();
        let header_line = lines.next().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "empty session file")
        })?;
        let header: PiSessionHeader = serde_json::from_str(header_line)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut entries = Vec::new();
        let mut next_id = 1u64;
        let mut skipped = false;
        for line in lines {
            if line.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<PiEntry>(line) {
                if entry.id >= next_id {
                    next_id = entry.id + 1;
                }
                entries.push(entry);
            } else {
                skipped = true;
            }
        }
        if checkpoint
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.entries != entries.len())
            || (checkpoint.is_some() && skipped)
        {
            return Err(invalid_data(
                "session checkpoint does not match its records",
            ));
        }

        Ok(Self {
            header,
            persisted_entries: entries.len(),
            entries,
            next_id,
            persisted_bytes: content.len() as u64,
            persisted_hash: Some(hash),
            persisted_path: Some(path.to_path_buf()),
            integrity_status,
            can_append: !skipped && content.ends_with('\n'),
            unverified_tail,
        })
    }

    /// Convert entries to provider Messages for the agent loop.
    pub fn messages(&self) -> Vec<Message> {
        self.entries
            .iter()
            .filter_map(|e| match &e.entry_type {
                PiEntryType::Message {
                    role,
                    content,
                    tool_call_id,
                } => {
                    if let Some(tid) = tool_call_id {
                        Some(Message::tool(tid, content.clone()))
                    } else {
                        Some(Message::new(*role, content.clone()))
                    }
                }
                _ => None,
            })
            .collect()
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Engine-owned todo list as of the last persisted entry.
    ///
    /// The engine holds todos in memory; this is what survives a restart.
    pub fn todos(&self) -> TodoState {
        self.entries
            .iter()
            .rev()
            .find_map(|entry| match &entry.entry_type {
                PiEntryType::Custom { extension, payload } if extension == TODOS_EXTENSION => {
                    serde_json::from_value(payload.clone()).ok()
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn message_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.entry_type, PiEntryType::Message { .. }))
            .count()
    }
}

/// Evidence that a session file ended mid-turn (crash, kill, closed terminal).
#[derive(Debug, Clone)]
pub struct InterruptInfo {
    /// Timestamp of the last activity that made it to disk.
    ///
    /// For a truncated trailing line this is the file mtime, not the last
    /// parsed entry — a crash mid-append can leave parsed rows hours old.
    pub last_activity: DateTime<Utc>,
    /// The last user prompt in the session (full text; excerpt at display).
    pub last_prompt: String,
    /// Parsed `message` entries in the file.
    pub message_count: usize,
    /// All parsed entries in the file.
    pub entry_count: usize,
}

fn entry_in_flight(entry: &PiEntryType) -> Option<bool> {
    match entry {
        PiEntryType::Message {
            tool_call_id: Some(_),
            ..
        } => Some(true),
        PiEntryType::Message { role, .. } => match role {
            Role::Assistant => Some(false),
            Role::User | Role::Tool => Some(true),
            Role::System => None,
        },
        PiEntryType::Custom { extension, .. }
            if extension == TOOL_CALL_EXTENSION || extension == TOOL_RESULT_EXTENSION =>
        {
            Some(true)
        }
        PiEntryType::Compaction { .. } => Some(true),
        PiEntryType::SessionInfo { key, .. } if key == INTERRUPTED_INFO_KEY => Some(true),
        PiEntryType::SessionInfo { key, .. } if key == CANCELLED_INFO_KEY => Some(false),
        _ => None,
    }
}

fn file_mtime_utc(path: &Path) -> Option<DateTime<Utc>> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(DateTime::<Utc>::from(modified))
}

/// Detect whether a session file was interrupted mid-turn.
///
/// Returns `Some(InterruptInfo)` when the trailing entries show a turn still
/// in flight, the file ends in an unparseable JSONL line, or a durable
/// `session_info` interrupted marker is present. Returns `None` for
/// cleanly-ended sessions, Esc-cancelled sessions marked as such, sessions
/// without any user prompt, and files whose header is missing or corrupt.
/// Never panics on truncated or garbage input.
pub fn session_interrupted(path: &Path) -> Option<InterruptInfo> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut lines = content.lines();
    let header_line = lines.next()?;
    serde_json::from_str::<PiSessionHeader>(header_line).ok()?;

    let mut entries: Vec<PiEntry> = Vec::new();
    let mut corrupt_tail = false;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<PiEntry>(line) {
            Ok(entry) => {
                entries.push(entry);
                corrupt_tail = false;
            }
            Err(_) => corrupt_tail = true,
        }
    }

    let last_prompt = entries
        .iter()
        .rev()
        .find_map(|entry| match &entry.entry_type {
            PiEntryType::Message {
                role: Role::User,
                content,
                tool_call_id: None,
            } => Some(content.clone()),
            _ => None,
        })?;

    let in_flight = entries
        .iter()
        .rev()
        .find_map(|entry| entry_in_flight(&entry.entry_type));
    if !corrupt_tail && in_flight != Some(true) {
        return None;
    }

    let last_activity = if corrupt_tail {
        file_mtime_utc(path).or_else(|| entries.last().map(|entry| entry.timestamp))?
    } else {
        entries.last().map(|entry| entry.timestamp)?
    };

    Some(InterruptInfo {
        last_activity,
        last_prompt,
        message_count: entries
            .iter()
            .filter(|entry| matches!(entry.entry_type, PiEntryType::Message { .. }))
            .count(),
        entry_count: entries.len(),
    })
}

fn clone_entry_type(et: &PiEntryType) -> PiEntryType {
    match et {
        PiEntryType::Message {
            role,
            content,
            tool_call_id,
        } => PiEntryType::Message {
            role: *role,
            content: content.clone(),
            tool_call_id: tool_call_id.clone(),
        },
        PiEntryType::ModelChange { from, to } => PiEntryType::ModelChange {
            from: from.clone(),
            to: to.clone(),
        },
        PiEntryType::ThinkingLevelChange { level } => PiEntryType::ThinkingLevelChange {
            level: level.clone(),
        },
        PiEntryType::Compaction { summary, cut_at } => PiEntryType::Compaction {
            summary: summary.clone(),
            cut_at: *cut_at,
        },
        PiEntryType::BranchSummary {
            from_session,
            at_entry,
        } => PiEntryType::BranchSummary {
            from_session: from_session.clone(),
            at_entry: *at_entry,
        },
        PiEntryType::SessionInfo { key, value } => PiEntryType::SessionInfo {
            key: key.clone(),
            value: value.clone(),
        },
        PiEntryType::Label { text } => PiEntryType::Label { text: text.clone() },
        PiEntryType::Custom { extension, payload } => PiEntryType::Custom {
            extension: extension.clone(),
            payload: payload.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn session_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test/project", "gpt-5.5");
        s.append_message(Role::User, "hello");
        s.append_message(Role::Assistant, "hi there");
        s.append_label("test-label");

        let path = s.save_jsonl(tmp.path()).unwrap();
        let loaded = PiSession::load_jsonl(&path).unwrap();
        assert_eq!(loaded.header.model, "gpt-5.5");
        assert_eq!(loaded.entry_count(), 3);
        assert_eq!(loaded.message_count(), 2);
    }

    #[test]
    fn saved_session_rejects_truncation_at_entry_boundary() {
        let tmp = TempDir::new().unwrap();
        let mut session = PiSession::new("/test/project", "gpt-5.5");
        session.append_message(Role::User, "first");
        session.append_message(Role::Assistant, "second");
        let path = session.save_jsonl(tmp.path()).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let prefix = content.split_inclusive('\n').take(2).collect::<String>();
        std::fs::write(&path, prefix).unwrap();
        let error = match PiSession::load_jsonl(&path) {
            Ok(_) => panic!("truncated session must not load"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("truncated-at-1 records"));
    }

    #[test]
    fn todo_state_round_trips_through_the_session() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test/project", "gpt-5.5");
        assert!(s.todos().items.is_empty());
        let todos = TodoState {
            items: vec![rx4::TodoItem {
                id: "one".to_string(),
                content: "wire the refresh".to_string(),
                status: rx4::TodoStatus::InProgress,
                creation_confidence: 60,
                completion_confidence: None,
                verification_attempts: 0,
            }],
        };
        s.append(PiEntryType::Custom {
            extension: TODOS_EXTENSION.to_string(),
            payload: serde_json::to_value(&todos).unwrap(),
        });

        let path = s.save_jsonl(tmp.path()).unwrap();
        let loaded = PiSession::load_jsonl(&path).unwrap();
        assert_eq!(loaded.todos().items.len(), 1);
        assert_eq!(loaded.todos().items[0].content, "wire the refresh");
        // Unrelated custom entries are not mistaken for todos.
        assert_eq!(
            PiSession::new("/test/project", "gpt-5.5")
                .todos()
                .items
                .len(),
            0
        );
    }

    #[test]
    fn incremental_appends_accumulate() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test/project", "gpt-5.5");
        s.append_message(Role::User, "one");
        let path = s.save_jsonl(tmp.path()).unwrap();
        let prefix = std::fs::read(&path).unwrap();

        s.append_message(Role::Assistant, "two");
        assert_eq!(s.save_jsonl(tmp.path()).unwrap(), path);
        assert!(std::fs::read(&path).unwrap().starts_with(&prefix));
        s.append_message(Role::User, "three");
        assert_eq!(s.save_jsonl(tmp.path()).unwrap(), path);

        let loaded = PiSession::load_jsonl(&path).unwrap();
        assert_eq!(loaded.entry_count(), 3);
        let contents: Vec<String> = loaded
            .messages()
            .iter()
            .map(|m| m.content.clone())
            .collect();
        assert_eq!(contents, vec!["one", "two", "three"]);
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            loaded_bytes(&path),
            "file must not carry stale bytes"
        );
    }

    #[test]
    fn changed_session_is_not_overwritten() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test/project", "gpt-5.5");
        s.append_message(Role::User, "one");
        let path = s.save_jsonl(tmp.path()).unwrap();

        let truncated = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, &truncated[..truncated.len() / 2]).unwrap();
        let damaged = std::fs::read(&path).unwrap();

        s.append_message(Role::Assistant, "two");
        assert!(s.save_jsonl(tmp.path()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), damaged);
    }

    #[test]
    fn integrity_checks_corruption_legacy_and_failed_save() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test/project", "test-model");
        s.append_message(Role::User, "first");
        let path = s.save_jsonl(tmp.path()).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert_eq!(
            PiSession::load_jsonl(&path).unwrap().integrity_status(),
            "verified"
        );

        let mut changed = original.clone();
        let pos = changed
            .windows(5)
            .position(|window| window == b"first")
            .unwrap();
        changed[pos] = b'F';
        std::fs::write(&path, &changed).unwrap();
        assert!(PiSession::load_jsonl(&path).is_err());
        std::fs::write(&path, &original).unwrap();
        let mut uncommitted = original.clone();
        uncommitted.extend_from_slice(b"{\"partial\":");
        std::fs::write(&path, &uncommitted).unwrap();
        let recovered = PiSession::load_jsonl(&path).unwrap();
        assert_eq!(recovered.messages()[0].content, "first");
        assert!(recovered.integrity_status().contains("partial"));
        assert_eq!(std::fs::read(&path).unwrap(), uncommitted);
        std::fs::write(&path, &original).unwrap();

        let checkpoint = checkpoint_path(&path);
        std::fs::remove_file(&checkpoint).unwrap();
        assert_eq!(
            PiSession::load_jsonl(&path).unwrap().integrity_status(),
            "unverified-legacy"
        );
        std::fs::create_dir(&checkpoint).unwrap();
        s.append_message(Role::Assistant, "second");
        assert!(s.save_jsonl(tmp.path()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn recovered_prefix_forks_without_modifying_unverified_tail() {
        let tmp = TempDir::new().unwrap();
        let mut session = PiSession::new("/test/project", "test-model");
        session.append_message(Role::User, "committed");
        let path = session.save_jsonl(tmp.path()).unwrap();
        let checkpoint = std::fs::read(checkpoint_path(&path)).unwrap();
        session.append_message(Role::Assistant, "unverified");
        let mut source = std::fs::read(&path).unwrap();
        serde_json::to_writer(&mut source, session.entries.last().unwrap()).unwrap();
        source.extend_from_slice(b"\n{\"partial\":");
        std::fs::write(&path, &source).unwrap();

        let mut recovered = PiSession::load_jsonl(&path).unwrap();
        assert_eq!(recovered.messages().len(), 1);
        assert!(recovered
            .integrity_status()
            .contains("verified to record 1"));
        assert!(recovered
            .integrity_status()
            .contains("1 complete unverified records"));
        assert!(recovered
            .integrity_status()
            .contains("partial trailing record"));
        recovered.append_message(Role::Assistant, "continued");
        let fork = recovered.save_jsonl(tmp.path()).unwrap();
        assert_ne!(fork, path);
        assert_eq!(std::fs::read(&path).unwrap(), source);
        assert_eq!(std::fs::read(checkpoint_path(&path)).unwrap(), checkpoint);
        let loaded = PiSession::load_jsonl(&fork).unwrap();
        assert_eq!(loaded.messages().len(), 2);
        assert_eq!(loaded.messages()[1].content, "continued");
        assert_eq!(loaded.integrity_status(), "verified");

        source[0] ^= 1;
        std::fs::write(&path, source).unwrap();
        assert!(PiSession::load_jsonl(&path).is_err());
    }

    #[test]
    fn oversized_append_leaves_previous_session_unchanged() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test/project", "test-model");
        s.append_message(Role::User, "first");
        let path = s.save_jsonl(tmp.path()).unwrap();
        let original = std::fs::read(&path).unwrap();
        s.append_message(Role::Assistant, "x".repeat(MAX_SESSION_BYTES));
        assert!(s.save_jsonl(tmp.path()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            PiSession::load_jsonl(&path).unwrap().integrity_status(),
            "verified"
        );
    }

    #[test]
    fn stale_loaded_writer_cannot_overwrite_new_entries() {
        let tmp = TempDir::new().unwrap();
        let mut first = PiSession::new("/test/project", "test-model");
        first.append_message(Role::User, "one");
        let path = first.save_jsonl(tmp.path()).unwrap();
        let mut second = PiSession::load_jsonl(&path).unwrap();
        first.append_message(Role::Assistant, "two");
        first.save_jsonl(tmp.path()).unwrap();
        second.append_message(Role::Assistant, "other");
        assert!(second.save_jsonl(tmp.path()).is_err());
        assert_eq!(
            PiSession::load_jsonl(&path).unwrap().messages()[1].content,
            "two"
        );
    }

    fn loaded_bytes(path: &Path) -> u64 {
        std::fs::read_to_string(path).unwrap().len() as u64
    }

    #[test]
    fn fork_preserves_prefix() {
        let mut s = PiSession::new("/test", "gpt-5.5");
        s.append_message(Role::User, "first");
        let fork_point = s.append_message(Role::Assistant, "second");
        s.append_message(Role::User, "third");

        let forked = s.fork(fork_point);
        assert_eq!(forked.entry_count(), 2);
    }

    #[test]
    fn messages_extracts_only_messages() {
        let mut s = PiSession::new("/test", "gpt-5.5");
        s.append_message(Role::User, "hello");
        s.append_model_change("gpt-5.5", "gpt-5.4-mini");
        s.append_message(Role::Assistant, "hi");

        let msgs = s.messages();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
    }

    fn saved(session: &mut PiSession, dir: &TempDir) -> std::path::PathBuf {
        session.save_jsonl(dir.path()).unwrap()
    }

    #[test]
    fn clean_session_is_not_interrupted() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test", "gpt-5.5");
        s.append_message(Role::User, "fix the tests");
        s.append_message(Role::Assistant, "done, all green");
        let path = saved(&mut s, &tmp);
        assert!(session_interrupted(&path).is_none());
    }

    #[test]
    fn trailing_user_prompt_is_interrupted() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test", "gpt-5.5");
        s.append_message(Role::User, "first ask");
        s.append_message(Role::Assistant, "first answer");
        s.append_message(Role::User, "second ask, never answered");
        let path = saved(&mut s, &tmp);
        let info = session_interrupted(&path).expect("prompt without reply");
        assert_eq!(info.last_prompt, "second ask, never answered");
        assert_eq!(info.message_count, 3);
        assert_eq!(info.entry_count, 3);
    }

    #[test]
    fn trailing_tool_call_is_interrupted() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test", "gpt-5.5");
        s.append_message(Role::User, "run the build");
        s.append_message(Role::Assistant, "running it");
        s.append(PiEntryType::Custom {
            extension: TOOL_CALL_EXTENSION.to_string(),
            payload: serde_json::json!({"id": "1", "name": "bash", "arguments": "{}"}),
        });
        let path = saved(&mut s, &tmp);
        let info = session_interrupted(&path).expect("tool call without result");
        assert_eq!(info.last_prompt, "run the build");
    }

    #[test]
    fn trailing_tool_result_is_interrupted() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test", "gpt-5.5");
        s.append_message(Role::User, "run the build");
        s.append_tool_result("1", "compiled");
        let path = saved(&mut s, &tmp);
        let info = session_interrupted(&path).expect("tool result awaiting model");
        assert_eq!(info.last_prompt, "run the build");
    }

    #[test]
    fn trailing_metadata_does_not_mask_clean_end() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test", "gpt-5.5");
        s.append_message(Role::User, "hello");
        s.append_message(Role::Assistant, "hi");
        s.append_model_change("gpt-5.5", "gpt-5.4-mini");
        let path = saved(&mut s, &tmp);
        assert!(session_interrupted(&path).is_none());
    }

    #[test]
    fn truncated_tail_is_interrupted() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test", "gpt-5.5");
        s.append_message(Role::User, "hello");
        s.append_message(Role::Assistant, "hi");
        let path = saved(&mut s, &tmp);

        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"{\"type\":\"message\",\"role\":\"assis")
            .unwrap();
        drop(file);

        let info = session_interrupted(&path).expect("truncated tail");
        assert_eq!(info.last_prompt, "hello");
        assert_eq!(info.entry_count, 2, "partial line must not count");
    }

    #[test]
    fn truncated_tail_uses_file_mtime_not_parsed_timestamp() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test", "gpt-5.5");
        s.append_message(Role::User, "hello");
        s.append_message(Role::Assistant, "hi");
        let old = Utc::now() - chrono::Duration::hours(48);
        for entry in &mut s.entries {
            entry.timestamp = old;
        }
        let path = saved(&mut s, &tmp);

        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"{\"type\":\"message\",\"role\":\"assis")
            .unwrap();
        file.sync_all().unwrap();
        drop(file);

        let info = session_interrupted(&path).expect("truncated tail");
        let age = Utc::now().signed_duration_since(info.last_activity);
        assert!(
            age < chrono::Duration::minutes(1),
            "corrupt tail must use mtime, got {age:?} (parsed stamp was 48h old)"
        );
    }

    #[test]
    fn mid_file_garbage_then_clean_assistant_is_not_interrupted() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test", "gpt-5.5");
        s.append_message(Role::User, "hello");
        s.append_message(Role::Assistant, "hi");
        let path = saved(&mut s, &tmp);

        let raw = std::fs::read_to_string(&path).unwrap();
        let mut lines: Vec<&str> = raw.lines().collect();
        lines.insert(2, "{ not json");
        std::fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();

        assert!(
            session_interrupted(&path).is_none(),
            "garbage that is not the trailing line must not classify the session"
        );
    }

    #[test]
    fn interrupted_marker_survives_clean_looking_rewrite() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test", "gpt-5.5");
        s.append_message(Role::User, "hello");
        s.append_message(Role::Assistant, "hi");
        s.append(PiEntryType::SessionInfo {
            key: INTERRUPTED_INFO_KEY.to_string(),
            value: "truncated-tail".to_string(),
        });
        let path = saved(&mut s, &tmp);
        let info = session_interrupted(&path).expect("durable interrupted marker");
        assert_eq!(info.last_prompt, "hello");
    }

    #[test]
    fn cancelled_marker_is_not_interrupted() {
        let tmp = TempDir::new().unwrap();
        let mut s = PiSession::new("/test", "gpt-5.5");
        s.append_message(Role::User, "hello");
        s.append(PiEntryType::SessionInfo {
            key: CANCELLED_INFO_KEY.to_string(),
            value: "esc".to_string(),
        });
        let path = saved(&mut s, &tmp);
        assert!(
            session_interrupted(&path).is_none(),
            "Esc-cancel must not look like a crash"
        );
    }

    #[test]
    fn sessions_without_prompt_or_header_are_ignored() {
        let tmp = TempDir::new().unwrap();

        let mut empty = PiSession::new("/test", "gpt-5.5");
        let path = saved(&mut empty, &tmp);
        assert!(session_interrupted(&path).is_none());

        let garbage = tmp.path().join("garbage.jsonl");
        std::fs::write(&garbage, "not json at all\n").unwrap();
        assert!(session_interrupted(&garbage).is_none());

        let empty_file = tmp.path().join("empty.jsonl");
        std::fs::write(&empty_file, "").unwrap();
        assert!(session_interrupted(&empty_file).is_none());
    }
}
