use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub path: String,
    pub tag: String,
    pub text: String,
    pub seen: BTreeSet<usize>,
}

#[derive(Debug, Default, Clone)]
pub struct SnapshotStore {
    by_path: HashMap<String, Snapshot>,
}

impl SnapshotStore {
    pub fn get(&self, path: &str) -> Option<&Snapshot> {
        self.by_path.get(path)
    }

    pub fn record(&mut self, path: &str, text: &str, seen: BTreeSet<usize>) -> Snapshot {
        let normalized = normalize_text(text);
        let snapshot = Snapshot {
            path: path.to_string(),
            tag: file_tag(&normalized),
            text: normalized,
            seen,
        };
        self.by_path.insert(path.to_string(), snapshot.clone());
        snapshot
    }

    pub fn forget(&mut self, path: &str) {
        self.by_path.remove(path);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Put {
        start: usize,
        end: usize,
        body: Vec<String>,
    },
    PutBefore {
        line: usize,
        body: Vec<String>,
    },
    PutAfter {
        line: usize,
        body: Vec<String>,
    },
    PutTail {
        body: Vec<String>,
    },
    Cut {
        start: usize,
        end: usize,
    },
    Rem,
    Mv {
        dest: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub path: String,
    pub tag: String,
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyResult {
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyError {
    pub message: String,
}

impl ApplyError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub trait TextFs {
    fn read(&self, path: &str) -> Result<String, String>;
    fn write(&mut self, path: &str, text: &str) -> Result<(), String>;
    fn remove(&mut self, path: &str) -> Result<(), String>;
    fn rename(&mut self, from: &str, to: &str) -> Result<(), String>;
}

#[cfg(test)]
#[derive(Default)]
struct MemoryFs {
    files: HashMap<String, String>,
}

#[cfg(test)]
impl MemoryFs {
    fn insert(&mut self, path: &str, text: impl Into<String>) {
        self.files.insert(path.to_string(), text.into());
    }

    fn get(&self, path: &str) -> Option<&String> {
        self.files.get(path)
    }
}

#[cfg(test)]
impl TextFs for MemoryFs {
    fn read(&self, path: &str) -> Result<String, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("file not found: {path}"))
    }

    fn write(&mut self, path: &str, text: &str) -> Result<(), String> {
        self.files.insert(path.to_string(), text.to_string());
        Ok(())
    }

    fn remove(&mut self, path: &str) -> Result<(), String> {
        self.files
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| format!("file not found: {path}"))
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        let text = self
            .files
            .remove(from)
            .ok_or_else(|| format!("file not found: {from}"))?;
        self.files.insert(to.to_string(), text);
        Ok(())
    }
}

pub struct DiskFs {
    root: PathBuf,
}

impl DiskFs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        resolve_workspace_path(&self.root, path)
    }
}

impl TextFs for DiskFs {
    fn read(&self, path: &str) -> Result<String, String> {
        let full = self.resolve(path)?;
        std::fs::read_to_string(&full).map_err(|error| format!("{error}"))
    }

    fn write(&mut self, path: &str, text: &str) -> Result<(), String> {
        let full = self.resolve(path)?;
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("mkdir failed: {error}"))?;
        }
        std::fs::write(&full, text).map_err(|error| format!("{error}"))
    }

    fn remove(&mut self, path: &str) -> Result<(), String> {
        let full = self.resolve(path)?;
        std::fs::remove_file(&full).map_err(|error| format!("{error}"))
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        let src = self.resolve(from)?;
        let dest = self.resolve(to)?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("mkdir failed: {error}"))?;
        }
        std::fs::rename(&src, &dest).map_err(|error| format!("{error}"))
    }
}

pub fn resolve_workspace_path(root: &Path, path: &str) -> Result<PathBuf, String> {
    let joined = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        root.join(path)
    };
    let root_norm = normalize_for_compare(root);
    let joined_norm = normalize_for_compare(&joined);
    if !joined_norm.starts_with(&root_norm) {
        return Err(format!("path escapes workspace: {path}"));
    }
    Ok(joined)
}

fn normalize_for_compare(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn normalize_text(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.replace("\r\n", "\n").replace('\r', "\n")
}

pub fn file_tag(text: &str) -> String {
    let normalized = normalize_text(text);
    format!("{:04X}", fnv1a32(normalized.as_bytes()) & 0xffff)
}

fn fnv1a32(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

pub fn sloppy_for_model(model: &str) -> bool {
    let lowered = model.to_ascii_lowercase();
    lowered.contains("kimi")
        || lowered.contains("moonshot")
        || lowered.contains("deepseek")
        || lowered.contains("ds-v")
        || lowered.contains("ds_v")
}

pub const SUMMARIZE_AFTER: usize = 160;
pub const SUMMARIZE_HEAD: usize = 80;
pub const SUMMARIZE_TAIL: usize = 40;

pub fn format_read(path: &str, text: &str, offset: usize, limit: usize) -> (String, Snapshot) {
    let normalized = normalize_text(text);
    let lines: Vec<&str> = if normalized.is_empty() {
        Vec::new()
    } else {
        normalized.split('\n').collect()
    };
    let start = offset.min(lines.len());
    let requested_end = start.saturating_add(limit).min(lines.len());
    let window_len = requested_end.saturating_sub(start);
    let mut seen = BTreeSet::new();
    let mut body = Vec::new();
    if lines.is_empty() {
        body.push("(empty file)".to_string());
    } else if window_len > SUMMARIZE_AFTER {
        let head_end = (start + SUMMARIZE_HEAD).min(requested_end);
        for (index, line) in lines[start..head_end].iter().enumerate() {
            let number = start + index + 1;
            seen.insert(number);
            body.push(format!("{number}:{line}"));
        }
        let tail_start = requested_end.saturating_sub(SUMMARIZE_TAIL).max(head_end);
        if tail_start > head_end {
            body.push(format!(
                "... elided lines {}-{}; re-read that window before editing it",
                head_end + 1,
                tail_start
            ));
        }
        for (index, line) in lines[tail_start..requested_end].iter().enumerate() {
            let number = tail_start + index + 1;
            seen.insert(number);
            body.push(format!("{number}:{line}"));
        }
    } else {
        for (index, line) in lines[start..requested_end].iter().enumerate() {
            let number = start + index + 1;
            seen.insert(number);
            body.push(format!("{number}:{line}"));
        }
    }
    let tag = file_tag(&normalized);
    let header = format!("[{path}#{tag}]");
    let output = std::iter::once(header)
        .chain(body)
        .collect::<Vec<_>>()
        .join("\n");
    let snapshot = Snapshot {
        path: path.to_string(),
        tag,
        text: normalized,
        seen,
    };
    (output, snapshot)
}

pub fn parse_patch(input: &str, sloppy: bool) -> Result<Vec<Section>, ApplyError> {
    let mut lines: Vec<&str> = input.lines().collect();
    if lines
        .first()
        .is_some_and(|line| line.trim() == "*** Begin Patch")
    {
        lines.remove(0);
    }
    if lines
        .last()
        .is_some_and(|line| line.trim() == "*** End Patch")
    {
        lines.pop();
    }
    let mut sections = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.is_empty() {
            index += 1;
            continue;
        }
        let Some((path, tag)) = parse_header(trimmed) else {
            return Err(ApplyError::new(format!(
                "expected [path#TAG], not {trimmed:?}. apply_patch / unified-diff is not the default edit path"
            )));
        };
        index += 1;
        let mut ops = Vec::new();
        while index < lines.len() {
            let op_line = lines[index].trim_end();
            if op_line.trim().is_empty() {
                index += 1;
                continue;
            }
            if parse_header(op_line.trim()).is_some() {
                break;
            }
            let (op, consumed) = parse_op(&lines, index, sloppy)?;
            ops.push(op);
            index = consumed;
        }
        if ops.is_empty() {
            return Err(ApplyError::new(format!(
                "{path}: section has no PUT/CUT/MV/REM operations"
            )));
        }
        sections.push(Section { path, tag, ops });
    }
    if sections.is_empty() {
        return Err(ApplyError::new(
            "hashline input must contain at least one [path#TAG] section",
        ));
    }
    Ok(sections)
}

fn parse_header(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let (path, tag) = inner.rsplit_once('#')?;
    if path.is_empty() || !is_tag(tag) {
        return None;
    }
    Some((path.to_string(), tag.to_ascii_uppercase()))
}

fn is_tag(tag: &str) -> bool {
    tag.len() == 4 && tag.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_op(lines: &[&str], start: usize, sloppy: bool) -> Result<(Op, usize), ApplyError> {
    let header = lines[start].trim();
    if header == "REM" {
        return Ok((Op::Rem, start + 1));
    }
    if let Some(dest) = header.strip_prefix("MV ") {
        let dest = unquote(dest.trim());
        if dest.is_empty() {
            return Err(ApplyError::new("MV requires a destination path"));
        }
        return Ok((Op::Mv { dest }, start + 1));
    }
    if let Some(rest) = header.strip_prefix("PUT ").or_else(|| {
        sloppy
            .then_some(header)
            .and_then(|line| line.strip_prefix("SWAP "))
    }) {
        if let Some(bodyless) = rest.strip_suffix(':') {
            if bodyless == ">$" {
                let (body, next) = take_body(lines, start + 1, sloppy)?;
                return Ok((Op::PutTail { body }, next));
            }
            if let Some(line) = parse_gap_line(bodyless, '<') {
                let (body, next) = take_body(lines, start + 1, sloppy)?;
                return Ok((Op::PutBefore { line, body }, next));
            }
            if let Some(line) = parse_gap_line(bodyless, '>') {
                let (body, next) = take_body(lines, start + 1, sloppy)?;
                return Ok((Op::PutAfter { line, body }, next));
            }
            if bodyless.ends_with('*') {
                return Err(ApplyError::new(
                    "block anchors (PUT N*:) are not in this slice; use PUT N.=M:",
                ));
            }
            let (start_line, end_line) = parse_range(bodyless, sloppy)?;
            let (body, next) = take_body(lines, start + 1, sloppy)?;
            return Ok((
                Op::Put {
                    start: start_line,
                    end: end_line,
                    body,
                },
                next,
            ));
        }
        return Err(ApplyError::new(format!(
            "PUT header must end with ':' and take +body rows: {header}"
        )));
    }
    if let Some(rest) = header.strip_prefix("CUT ").or_else(|| {
        sloppy
            .then_some(header)
            .and_then(|line| line.strip_prefix("DEL "))
    }) {
        if rest.ends_with('*') {
            return Err(ApplyError::new(
                "block anchors (CUT N*) are not in this slice; use CUT N.=M",
            ));
        }
        let (start_line, end_line) = parse_range(rest, sloppy)?;
        return Ok((
            Op::Cut {
                start: start_line,
                end: end_line,
            },
            start + 1,
        ));
    }
    Err(ApplyError::new(format!(
        "unknown hashline op {header:?}; use PUT/CUT/MV/REM (not apply_patch)"
    )))
}

fn parse_gap_line(spec: &str, marker: char) -> Option<usize> {
    spec.strip_prefix(marker)?.parse().ok()
}

fn parse_range(spec: &str, sloppy: bool) -> Result<(usize, usize), ApplyError> {
    let spec = spec.trim();
    if let Some((start, end)) = spec.split_once(".=") {
        return parse_bounds(start, end);
    }
    if sloppy {
        if let Some((start, end)) = spec.split_once('-') {
            return parse_bounds(start, end);
        }
    }
    if let Ok(line) = spec.parse::<usize>() {
        if line == 0 {
            return Err(ApplyError::new("line numbers are 1-based"));
        }
        return Ok((line, line));
    }
    Err(ApplyError::new(format!(
        "invalid range {spec:?}; expected N.=M"
    )))
}

fn parse_bounds(start: &str, end: &str) -> Result<(usize, usize), ApplyError> {
    let start: usize = start
        .parse()
        .map_err(|_| ApplyError::new("invalid start line"))?;
    let end: usize = end
        .parse()
        .map_err(|_| ApplyError::new("invalid end line"))?;
    if start == 0 || end == 0 {
        return Err(ApplyError::new("line numbers are 1-based"));
    }
    if start > end {
        return Err(ApplyError::new(format!("reversed range {start}.={end}")));
    }
    Ok((start, end))
}

fn take_body(
    lines: &[&str],
    mut index: usize,
    sloppy: bool,
) -> Result<(Vec<String>, usize), ApplyError> {
    let mut body = Vec::new();
    while index < lines.len() {
        let line = lines[index];
        if line.starts_with('+') {
            body.push(decode_body_row(line));
            index += 1;
            continue;
        }
        if sloppy
            && !line.starts_with('[')
            && !is_op_header(line.trim())
            && line.trim() != "*** End Patch"
        {
            body.push(line.to_string());
            index += 1;
            continue;
        }
        break;
    }
    if body.is_empty() {
        return Err(ApplyError::new(
            "PUT ...: requires at least one +body row (use + alone for a blank line)",
        ));
    }
    Ok((body, index))
}

fn is_op_header(line: &str) -> bool {
    line == "REM"
        || line.starts_with("PUT ")
        || line.starts_with("CUT ")
        || line.starts_with("MV ")
        || line.starts_with("SWAP ")
        || line.starts_with("DEL ")
}

fn decode_body_row(line: &str) -> String {
    line[1..].to_string()
}

fn unquote(value: &str) -> String {
    if (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
        || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

pub fn apply_patch(
    store: &mut SnapshotStore,
    fs: &mut impl TextFs,
    sections: &[Section],
) -> Result<ApplyResult, ApplyError> {
    let mut prepared = Vec::new();
    for section in sections {
        prepared.push(prepare_section(store, fs, section)?);
    }
    let mut outputs = Vec::new();
    for prepared in prepared {
        outputs.push(commit_section(store, fs, prepared)?);
    }
    if let Some(last) = outputs.last_mut() {
        last.push_str("\nDiagnostics: unavailable");
    }
    Ok(ApplyResult { outputs })
}

struct PreparedSection {
    path: String,
    newline: &'static str,
    after: String,
    rem: bool,
    dest: Option<String>,
}

fn prepare_section(
    store: &SnapshotStore,
    fs: &impl TextFs,
    section: &Section,
) -> Result<PreparedSection, ApplyError> {
    let snapshot = store.get(&section.path).ok_or_else(|| {
        ApplyError::new(format!(
            "{}: unseen file; read it before editing",
            section.path
        ))
    })?;
    if snapshot.tag != section.tag {
        return Err(ApplyError::new(format!(
            "{}: stale tag {} (latest is {}); re-read",
            section.path, section.tag, snapshot.tag
        )));
    }
    let live = fs
        .read(&section.path)
        .map_err(|error| ApplyError::new(format!("{}: {error}", section.path)))?;
    let newline = if live.contains("\r\n") { "\r\n" } else { "\n" };
    let live_norm = normalize_text(&live);
    if file_tag(&live_norm) != section.tag {
        return Err(ApplyError::new(format!(
            "{}: stale tag {}; live file no longer matches the tagged snapshot; re-read",
            section.path, section.tag
        )));
    }
    let mut lines: Vec<String> = if live_norm.is_empty() {
        Vec::new()
    } else {
        live_norm.split('\n').map(str::to_string).collect()
    };
    let original = lines.clone();
    let mut dest = None;
    let mut rem = false;
    for op in &section.ops {
        match op {
            Op::Put { start, end, body } => {
                require_seen(snapshot, *start, *end)?;
                splice(&mut lines, *start, *end, body)?;
            }
            Op::PutBefore { line, body } => {
                require_seen(snapshot, *line, *line)?;
                insert_at(&mut lines, *line, body, false)?;
            }
            Op::PutAfter { line, body } => {
                require_seen(snapshot, *line, *line)?;
                insert_at(&mut lines, *line, body, true)?;
            }
            Op::PutTail { body } => {
                if snapshot.seen.is_empty() && !original.is_empty() {
                    return Err(ApplyError::new(format!(
                        "{}: unseen file window; re-read before appending",
                        section.path
                    )));
                }
                lines.extend(body.iter().cloned());
            }
            Op::Cut { start, end } => {
                require_seen(snapshot, *start, *end)?;
                splice(&mut lines, *start, *end, &[])?;
            }
            Op::Rem => rem = true,
            Op::Mv { dest: next } => dest = Some(next.clone()),
        }
    }
    let after = if rem {
        String::new()
    } else {
        join_lines(&lines)
    };
    if !rem && after == live_norm {
        return Err(ApplyError::new(format!(
            "{}: no-op edit; hashline hard-fails byte-identical patches",
            section.path
        )));
    }
    let _ = original;
    Ok(PreparedSection {
        path: section.path.clone(),
        newline,
        after,
        rem,
        dest,
    })
}

fn commit_section(
    store: &mut SnapshotStore,
    fs: &mut impl TextFs,
    prepared: PreparedSection,
) -> Result<String, ApplyError> {
    if prepared.rem {
        fs.remove(&prepared.path)
            .map_err(|error| ApplyError::new(format!("{}: {error}", prepared.path)))?;
        store.forget(&prepared.path);
        return Ok(format!("[{}#----]\nREM", prepared.path));
    }
    let disk_text = restore_newlines(&prepared.after, prepared.newline);
    let dest = prepared
        .dest
        .clone()
        .unwrap_or_else(|| prepared.path.clone());
    if dest != prepared.path {
        fs.write(&prepared.path, &disk_text)
            .map_err(|error| ApplyError::new(format!("{}: {error}", prepared.path)))?;
        fs.rename(&prepared.path, &dest)
            .map_err(|error| ApplyError::new(format!("{}: {error}", prepared.path)))?;
        store.forget(&prepared.path);
    } else {
        fs.write(&dest, &disk_text)
            .map_err(|error| ApplyError::new(format!("{dest}: {error}")))?;
    }
    let (output, snapshot) = format_read(&dest, &prepared.after, 0, usize::MAX);
    store.by_path.insert(dest, snapshot);
    Ok(output)
}

fn require_seen(snapshot: &Snapshot, start: usize, end: usize) -> Result<(), ApplyError> {
    for line in start..=end {
        if !snapshot.seen.contains(&line) {
            return Err(ApplyError::new(format!(
                "{}: unseen line {line}; re-read that window before hunking it",
                snapshot.path
            )));
        }
    }
    Ok(())
}

fn splice(
    lines: &mut Vec<String>,
    start: usize,
    end: usize,
    body: &[String],
) -> Result<(), ApplyError> {
    if start == 0 || end == 0 || start > lines.len() || end > lines.len() {
        return Err(ApplyError::new(format!(
            "range {start}.={end} is outside the file ({} lines)",
            lines.len()
        )));
    }
    lines.splice(start - 1..end, body.iter().cloned());
    Ok(())
}

fn insert_at(
    lines: &mut Vec<String>,
    line: usize,
    body: &[String],
    after: bool,
) -> Result<(), ApplyError> {
    if line == 0 || line > lines.len() {
        return Err(ApplyError::new(format!(
            "anchor {line} is outside the file ({} lines)",
            lines.len()
        )));
    }
    let at = if after { line } else { line - 1 };
    for (offset, row) in body.iter().enumerate() {
        lines.insert(at + offset, row.clone());
    }
    Ok(())
}

fn join_lines(lines: &[String]) -> String {
    lines.join("\n")
}

fn restore_newlines(text: &str, newline: &str) -> String {
    if newline == "\n" {
        text.to_string()
    } else {
        text.replace('\n', newline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded(text: &str) -> (SnapshotStore, MemoryFs, String) {
        let mut fs = MemoryFs::default();
        fs.insert("greet.py", text);
        let mut store = SnapshotStore::default();
        let (formatted, snapshot) = format_read("greet.py", text, 0, 200);
        store.by_path.insert("greet.py".into(), snapshot);
        let tag = formatted
            .lines()
            .next()
            .and_then(|line| line.rsplit_once('#'))
            .map(|(_, tag)| tag.trim_end_matches(']').to_string())
            .unwrap();
        (store, fs, tag)
    }

    #[test]
    fn hashline_put_replaces_inclusive_range() {
        let source = "one\ntwo\nthree\n";
        let (mut store, mut fs, tag) = seeded(source);
        let patch = parse_patch(&format!("[greet.py#{tag}]\nPUT 2.=2:\n+TWO\n"), false).unwrap();
        let result = apply_patch(&mut store, &mut fs, &patch).unwrap();
        assert_eq!(fs.get("greet.py").unwrap(), "one\nTWO\nthree\n");
        assert!(result.outputs[0].contains("[greet.py#"));
        assert!(result.outputs[0].contains("2:TWO"));
        assert!(result.outputs[0].contains("Diagnostics: unavailable"));
    }

    #[test]
    fn stale_tag_is_rejected_fail_closed() {
        let (mut store, mut fs, tag) = seeded("alpha\n");
        fs.insert("greet.py", "changed\n");
        let patch = parse_patch(&format!("[greet.py#{tag}]\nPUT 1.=1:\n+beta\n"), false).unwrap();
        let error = apply_patch(&mut store, &mut fs, &patch).unwrap_err();
        assert!(error.message.contains("stale tag"), "{}", error.message);
        assert_eq!(fs.get("greet.py").unwrap(), "changed\n");
    }

    #[test]
    fn noop_put_is_hard_fail() {
        let (mut store, mut fs, tag) = seeded("same\n");
        let patch = parse_patch(&format!("[greet.py#{tag}]\nPUT 1.=1:\n+same\n"), false).unwrap();
        let error = apply_patch(&mut store, &mut fs, &patch).unwrap_err();
        assert!(error.message.contains("no-op"), "{}", error.message);
        assert_eq!(fs.get("greet.py").unwrap(), "same\n");
    }

    #[test]
    fn unseen_lines_cannot_be_hunked() {
        let mut text = String::new();
        for i in 1..=200 {
            text.push_str(&format!("line-{i}\n"));
        }
        let (mut store, mut fs, tag) = seeded(&text);
        assert!(!store.get("greet.py").unwrap().seen.contains(&100));
        let patch = parse_patch(
            &format!("[greet.py#{tag}]\nPUT 100.=100:\n+changed\n"),
            false,
        )
        .unwrap();
        let error = apply_patch(&mut store, &mut fs, &patch).unwrap_err();
        assert!(
            error.message.contains("unseen line 100"),
            "{}",
            error.message
        );
    }

    #[test]
    fn sloppy_kimi_class_accepts_dash_range() {
        assert!(sloppy_for_model("moonshot/kimi-k2.5"));
        assert!(sloppy_for_model("deepseek-v3"));
        assert!(!sloppy_for_model("gpt-5.6-sol"));
        let (mut store, mut fs, tag) = seeded("a\nb\nc\n");
        let patch = parse_patch(&format!("[greet.py#{tag}]\nPUT 2-2:\nb-prime\n"), true).unwrap();
        apply_patch(&mut store, &mut fs, &patch).unwrap();
        assert_eq!(fs.get("greet.py").unwrap(), "a\nb-prime\nc\n");
    }

    #[test]
    fn apply_patch_envelope_is_not_the_edit_language() {
        let error =
            parse_patch("*** Begin Patch\n*** Update File: x\n@@\n-a\n+b\n", false).unwrap_err();
        assert!(error.message.contains("apply_patch"), "{}", error.message);
    }
}
