//! Linux atomic, durable publication without replacing existing/manual files.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io::Write,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Temporary {
    pub file: File,
    pub path: PathBuf,
}
impl Temporary {
    pub fn new(parent: &Path) -> Result<Self> {
        std::fs::create_dir_all(parent)?;
        let time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = parent.join(format!(
            ".pocket-tts-partial-{}-{time}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        Ok(Self { file, path })
    }
    pub fn publish(self, target: &Path) -> Result<()> {
        ensure!(
            self.path.parent() == target.parent(),
            "atomic publication requires same directory"
        );
        self.file.sync_all()?;
        let source = CString::new(self.path.as_os_str().as_bytes())?;
        let dest = CString::new(target.as_os_str().as_bytes())?;
        // Both paths are valid NUL-terminated strings. RENAME_NOREPLACE prevents
        // races with manual installs; no pre-check/rename overwrite window.
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                dest.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error())
                .context(format!("publish without overwrite: {}", target.display()));
        }
        File::open(target.parent().context("target parent")?)?.sync_all()?;
        Ok(())
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
pub fn write_new(target: &Path, bytes: &[u8]) -> Result<()> {
    let mut t = Temporary::new(target.parent().context("target parent")?)?;
    t.file.write_all(bytes)?;
    t.publish(target)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_never_overwrites_and_cleans_owned_partial() {
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("manual");
        std::fs::write(&target, b"manual").unwrap();
        let mut t = Temporary::new(d.path()).unwrap();
        let partial = t.path.clone();
        t.file.write_all(b"replacement").unwrap();
        assert!(t.publish(&target).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"manual");
        assert!(!partial.exists());
        let other = d.path().join("installed");
        write_new(&other, b"complete").unwrap();
        assert_eq!(std::fs::read(other).unwrap(), b"complete");
    }
}
