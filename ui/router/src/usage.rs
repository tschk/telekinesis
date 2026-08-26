//! Local request/token activity per provider. Not invoices.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTotals {
    #[serde(default)]
    pub requests: u64,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_at: Option<u64>,
}

impl ProviderTotals {
    pub fn add(&mut self, event: &UsageEvent) {
        self.requests = self.requests.saturating_add(1);
        self.input_tokens = self.input_tokens.saturating_add(event.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(event.output_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(event.cache_read_tokens);
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(event.cache_write_tokens);
        self.last_at = Some(now_secs());
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsageEvent {
    pub provider_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl UsageEvent {
    pub fn new(provider_id: impl Into<String>, input: u64, output: u64) -> Self {
        Self {
            provider_id: provider_id.into(),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageLog {
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderTotals>,
}

impl UsageLog {
    pub fn record(&mut self, event: &UsageEvent) {
        let id = event.provider_id.trim();
        if id.is_empty() {
            return;
        }
        self.providers.entry(id.to_string()).or_default().add(event);
    }

    pub fn total_requests(&self) -> u64 {
        self.providers.values().map(|row| row.requests).sum()
    }

    pub fn total_tokens(&self) -> u64 {
        self.providers
            .values()
            .map(|row| {
                row.input_tokens
                    .saturating_add(row.output_tokens)
                    .saturating_add(row.cache_read_tokens)
                    .saturating_add(row.cache_write_tokens)
            })
            .sum()
    }
}

pub fn usage_path() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("TELEKINESIS_USAGE_PATH") {
        return Some(PathBuf::from(explicit));
    }
    dirs::home_dir().map(|home| home.join(".telekinesis").join("usage.json"))
}

pub fn load_log() -> UsageLog {
    let Some(path) = usage_path() else {
        return UsageLog::default();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

static USAGE_MUTEX: Mutex<()> = Mutex::new(());

#[cfg(unix)]
fn lock_exclusive(file: &File) -> std::io::Result<()> {
    use std::os::unix::io::AsRawFd;
    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    const LOCK_EX: i32 = 2;
    let rc = unsafe { flock(file.as_raw_fd(), LOCK_EX) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn lock_exclusive(_file: &File) -> std::io::Result<()> {
    Ok(())
}

fn lock_usage(path: &Path) -> Option<File> {
    let lock_path = path.with_extension("json.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
        .ok()?;
    let _ = lock_exclusive(&file);
    Some(file)
}

fn write_log(path: &Path, log: &UsageLog) {
    let Ok(raw) = serde_json::to_string_pretty(log) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, &raw).is_ok() && std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::write(path, raw);
    }
}

pub fn record_event(event: &UsageEvent) -> UsageLog {
    let _in_process = USAGE_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(path) = usage_path() else {
        let mut log = UsageLog::default();
        log.record(event);
        return log;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _file_lock = lock_usage(&path);
    let mut log: UsageLog = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    log.record(event);
    write_log(&path, &log);
    log
}

pub fn record_turn(
    provider_id: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
) -> UsageLog {
    record_event(&UsageEvent {
        provider_id: provider_id.to_string(),
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
    })
}

pub fn format_table(log: &UsageLog) -> String {
    if log.providers.is_empty() {
        return "Usage (local activity, not invoices)\n  no requests recorded yet.".to_string();
    }
    let mut lines = vec!["Usage (local activity, not invoices)".to_string()];
    for (id, row) in &log.providers {
        lines.push(format!(
            "  {id:<22} {req} req  {inn} in  {out} out  cache {cr}/{cw}",
            req = row.requests,
            inn = row.input_tokens,
            out = row.output_tokens,
            cr = row.cache_read_tokens,
            cw = row.cache_write_tokens,
        ));
    }
    lines.push(format!(
        "  {:<22} {} req  {} tokens",
        "total",
        log.total_requests(),
        log.total_tokens()
    ));
    lines.join("\n")
}

pub fn format_short(log: &UsageLog) -> String {
    if log.providers.is_empty() {
        return "0 req".to_string();
    }
    format!("{} req · {} tok", log.total_requests(), log.total_tokens())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static ISOLATE: AtomicU64 = AtomicU64::new(0);
    static TEST_ENV: Mutex<()> = Mutex::new(());

    fn with_temp_log<F: FnOnce()>(f: F) {
        let _guard = TEST_ENV
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let n = ISOLATE.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("tk-usage-test-{}-{n}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("usage.json");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("json.lock"));
        let _ = std::fs::remove_file(path.with_extension("json.tmp"));
        std::env::set_var("TELEKINESIS_USAGE_PATH", &path);
        f();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("json.lock"));
        let _ = std::fs::remove_file(path.with_extension("json.tmp"));
        std::env::remove_var("TELEKINESIS_USAGE_PATH");
    }

    #[test]
    fn records_per_provider_and_formats() {
        with_temp_log(|| {
            record_turn("clinepass", 10, 4, 1, 0);
            record_turn("clinepass", 5, 2, 0, 0);
            record_turn("openai", 3, 1, 0, 0);
            let log = load_log();
            assert_eq!(log.providers["clinepass"].requests, 2);
            assert_eq!(log.providers["clinepass"].input_tokens, 15);
            assert_eq!(log.providers["openai"].requests, 1);
            let table = format_table(&log);
            assert!(table.contains("clinepass"));
            assert!(table.contains("openai"));
            assert!(table.contains("3 req"));
            assert_eq!(format_short(&log), "3 req · 26 tok");
        });
    }

    #[test]
    fn concurrent_writers_keep_all_events() {
        with_temp_log(|| {
            let threads: Vec<_> = (0..8)
                .map(|_| {
                    std::thread::spawn(|| {
                        for _ in 0..25 {
                            record_turn("clinepass", 1, 0, 0, 0);
                        }
                    })
                })
                .collect();
            for thread in threads {
                thread.join().unwrap();
            }
            let log = load_log();
            assert_eq!(log.providers["clinepass"].requests, 200);
            assert_eq!(log.providers["clinepass"].input_tokens, 200);
        });
    }

    #[test]
    fn skips_empty_provider_id() {
        let mut log = UsageLog::default();
        log.record(&UsageEvent::new("  ", 1, 1));
        assert!(log.providers.is_empty());
    }
}
