//! Per-agent file-freshness tracking.
//!
//! When the agent calls a content-read tool (`read_file`, `read_docx`, …),
//! the runner records the file's `mtime` at read time. Before each new user
//! turn, the runner asks the tracker which previously-read files have been
//! modified or removed since — those names get prepended to the user message
//! as a one-line hint, so the LLM knows it should re-read instead of
//! answering from a stale snapshot.
//!
//! The tracker is best-effort: any IO failure (path can't be canonicalized,
//! metadata unavailable) silently no-ops rather than blocking tool execution.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::Mutex;

#[derive(Debug, Clone, Default)]
pub struct FreshnessTracker {
    reads: Arc<Mutex<HashMap<(String, PathBuf), SystemTime>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleReason {
    Modified,
    Removed,
}

#[derive(Debug, Clone)]
pub struct StaleEntry {
    pub path: PathBuf,
    pub reason: StaleReason,
}

impl FreshnessTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `path` was read for `agent_id`. Captures the file's
    /// current mtime so a later modification can be detected. Silently does
    /// nothing if the path can't be canonicalized or its metadata can't be
    /// read — this is best-effort instrumentation and never blocks tool
    /// execution.
    pub async fn record_read(&self, agent_id: &str, path: &Path) {
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => return,
        };
        let mtime = match std::fs::metadata(&canonical).and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => return,
        };
        let mut reads = self.reads.lock().await;
        reads.insert((agent_id.to_string(), canonical), mtime);
    }

    /// Files this agent has read whose current mtime differs from the
    /// recorded one (`Modified`) or which can no longer be read at the
    /// canonical path (`Removed` — covers deletions and renames). The set
    /// returned here is what should be flagged to the LLM at the start of
    /// the next user turn.
    pub async fn stale_paths(&self, agent_id: &str) -> Vec<StaleEntry> {
        let reads = self.reads.lock().await;
        let mut out = Vec::new();
        for ((aid, path), recorded) in reads.iter() {
            if aid != agent_id {
                continue;
            }
            match std::fs::metadata(path).and_then(|m| m.modified()) {
                Err(_) => out.push(StaleEntry {
                    path: path.clone(),
                    reason: StaleReason::Removed,
                }),
                Ok(current) if current > *recorded => out.push(StaleEntry {
                    path: path.clone(),
                    reason: StaleReason::Modified,
                }),
                Ok(_) => {}
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Disposable test directory rooted in the OS temp dir. Cleaned up on
    /// drop. Pattern matches `core/sandbox.rs::tests::tmp_dir` so we don't
    /// pull in an extra crate just for tests.
    struct Tmp(PathBuf);

    impl Tmp {
        fn new(prefix: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "processfox_freshness_{prefix}_{}_{}",
                std::process::id(),
                n
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write(path: &Path, body: &str) {
        std::fs::write(path, body).unwrap();
    }

    #[tokio::test]
    async fn record_then_unchanged_is_not_stale() {
        let dir = Tmp::new("unchanged");
        let f = dir.path().join("a.md");
        write(&f, "v1");

        let t = FreshnessTracker::new();
        t.record_read("agent1", &f).await;
        let stale = t.stale_paths("agent1").await;
        assert!(stale.is_empty(), "fresh file should not be stale");
    }

    #[tokio::test]
    async fn external_modification_is_detected() {
        let dir = Tmp::new("modified");
        let f = dir.path().join("a.md");
        write(&f, "v1");

        let t = FreshnessTracker::new();
        t.record_read("agent1", &f).await;

        // mtime resolution on macOS is typically nanoseconds but on some
        // filesystems only seconds; sleep past 1s to guarantee a different
        // mtime in CI.
        std::thread::sleep(Duration::from_millis(1100));
        write(&f, "v2");

        let stale = t.stale_paths("agent1").await;
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].reason, StaleReason::Modified);
    }

    #[tokio::test]
    async fn deletion_is_detected_as_removed() {
        let dir = Tmp::new("removed");
        let f = dir.path().join("a.md");
        write(&f, "v1");

        let t = FreshnessTracker::new();
        t.record_read("agent1", &f).await;
        std::fs::remove_file(&f).unwrap();

        let stale = t.stale_paths("agent1").await;
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].reason, StaleReason::Removed);
    }

    #[tokio::test]
    async fn agent_isolation() {
        let dir = Tmp::new("isolation");
        let f = dir.path().join("a.md");
        write(&f, "v1");

        let t = FreshnessTracker::new();
        t.record_read("agent_a", &f).await;
        std::thread::sleep(Duration::from_millis(1100));
        write(&f, "v2");

        let stale_a = t.stale_paths("agent_a").await;
        let stale_b = t.stale_paths("agent_b").await;
        assert_eq!(stale_a.len(), 1);
        assert!(
            stale_b.is_empty(),
            "agent_b never read this file, should not see staleness from it"
        );
    }

    #[tokio::test]
    async fn nonexistent_path_record_is_silent() {
        let dir = Tmp::new("nope");
        let phantom = dir.path().join("nope.md");

        let t = FreshnessTracker::new();
        // Should not panic and should not record anything.
        t.record_read("agent1", &phantom).await;
        assert!(t.stale_paths("agent1").await.is_empty());
    }
}
