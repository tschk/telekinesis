//! Canonical-workspace admission retained until engine-owned execution settles.
use super::workers::Owner;
use super::*;
struct Lease {
    owner: Owner,
    _file: Option<std::fs::File>,
}
#[derive(Default)]
pub(super) struct Ownership {
    leases: HashMap<PathBuf, Lease>,
}
pub(super) fn shared() -> Arc<Mutex<Ownership>> {
    static OWNERS: OnceLock<Arc<Mutex<Ownership>>> = OnceLock::new();
    OWNERS
        .get_or_init(|| Arc::new(Mutex::new(Ownership::default())))
        .clone()
}
impl Ownership {
    /// Caller retains this mutex through engine reservation, making spawn admission atomic.
    pub(super) fn admit(&mut self, path: &Path, owner: &Owner) -> Result<bool> {
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        if let Some(lease) = self.leases.get(&path) {
            return if &lease.owner == owner {
                Ok(false)
            } else {
                Err("Workspace is busy; another run owns writes or cleanup is unproved.".into())
            };
        }
        #[cfg(test)]
        let dir =
            std::env::temp_dir().join(format!("telekinesis-test-ownership-{}", std::process::id()));
        #[cfg(not(test))]
        let dir = crate::host::config_home()
            .ok_or("No private ownership lock directory available.")?
            .join("workspace-locks");
        if std::fs::symlink_metadata(&dir).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Workspace ownership directory must not be a symlink.".into());
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        // Locks stay outside the project so git clean cannot unlink an active lease.
        let lock = dir.join(format!("{}.lock", workspace_key(&path)?));
        if std::fs::symlink_metadata(&lock).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Workspace ownership lock must not be a symlink.".into());
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock)
            .map_err(|e| e.to_string())?;
        file.try_lock().map_err(|_| {
            "Workspace is owned by another host; wait for real settlement.".to_string()
        })?;
        if file.metadata().map_err(|e| e.to_string())?.len() != 0 {
            return Err("Workspace has an unproved prior owner; reconcile that execution before admitting new writes.".into());
        }
        use std::io::Write;
        let mut record = &file;
        record
            .write_all(b"owned\n")
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        self.leases.insert(
            path.clone(),
            Lease {
                owner: owner.clone(),
                _file: Some(file),
            },
        );
        Ok(true)
    }
    pub(super) fn restored(&mut self, path: &Path, owner: Owner) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(lease) = self.leases.get_mut(&path) {
            if owner.unreconciled || lease.owner != owner {
                // Restored uncertainty adds a hold; unrelated terminal metadata
                // must not clear an existing live owner's descriptor or marker.
                lease.owner.unreconciled = true;
            }
            return;
        }
        if self.admit(&path, &owner).is_err() {
            self.leases
                .entry(path.to_path_buf())
                .or_insert(Lease { owner, _file: None });
        }
    }
    pub(super) fn retain(&mut self, mut keep: impl FnMut(&Owner) -> bool) -> Result<()> {
        let mut error = None;
        self.leases.retain(|_, lease| {
            if keep(&lease.owner) {
                return true;
            }
            let Some(file) = &lease._file else {
                return true;
            };
            match file.set_len(0).and_then(|_| file.sync_all()) {
                Ok(()) => false,
                Err(e) => {
                    lease.owner.unreconciled = true;
                    error = Some(e.to_string());
                    true
                }
            }
        });
        error.map_or(Ok(()), Err)
    }
    pub(super) fn release_failed_reservation(&mut self, path: &Path, owner: &Owner) -> Result<()> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !self.leases.get(&path).is_some_and(|l| &l.owner == owner) {
            return Ok(());
        }
        let lease = self.leases.get_mut(&path).unwrap();
        let Some(file) = &lease._file else {
            return Err("No live admission owner to release.".into());
        };
        if let Err(e) = file.set_len(0).and_then(|_| file.sync_all()) {
            lease.owner.unreconciled = true;
            return Err(e.to_string());
        }
        self.leases.remove(&path);
        Ok(())
    }
}

fn workspace_key(path: &Path) -> Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
        Ok(format!("{}-{}", metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err("Workspace writer ownership is unsupported on this platform.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_owners_share_one_atomic_os_lock_and_no_tool_drop_releases_it() {
        let dir = tempfile::tempdir().unwrap();
        let a = Owner {
            root: "a".into(),
            request: "task".into(),
            unreconciled: false,
        };
        let b = Owner {
            root: "b".into(),
            request: "task".into(),
            unreconciled: false,
        };
        let mut first = Ownership::default();
        let mut second = Ownership::default();
        assert!(first.admit(dir.path(), &a).unwrap());
        assert!(!first.admit(dir.path(), &a).unwrap());
        assert!(second.admit(dir.path(), &b).is_err());
        first.retain(|_| true).unwrap();
        assert!(second.admit(dir.path(), &b).is_err());
        first.retain(|_| false).unwrap();
        assert!(second.admit(dir.path(), &b).unwrap());
    }
    #[test]
    fn interrupted_root_marker_cannot_be_reused_as_a_worker_request() {
        let dir = tempfile::tempdir().unwrap();
        let mut gate = Ownership::default();
        let unknown = Owner {
            root: "same-root".into(),
            request: "interrupted-root".into(),
            unreconciled: true,
        };
        gate.restored(dir.path(), unknown);
        let proposed = Owner {
            root: "same-root".into(),
            request: "interrupted-root".into(),
            unreconciled: false,
        };
        assert!(gate.admit(dir.path(), &proposed).is_err());
    }
    #[test]
    fn dropping_owner_keeps_durable_hold_until_external_reconciliation() {
        let dir = tempfile::tempdir().unwrap();
        let a = Owner {
            root: "a".into(),
            request: "task".into(),
            unreconciled: false,
        };
        let mut first = Ownership::default();
        first.admit(dir.path(), &a).unwrap();
        drop(first);
        let mut next = Ownership::default();
        let b = Owner {
            root: "b".into(),
            request: "task".into(),
            unreconciled: false,
        };
        assert!(next.admit(dir.path(), &b).is_err());
        // This fixture never started execution. Keep its workspace alive so an
        // unrelated temp directory cannot reuse its inode during the same run.
        let _ = dir.keep();
    }
}
