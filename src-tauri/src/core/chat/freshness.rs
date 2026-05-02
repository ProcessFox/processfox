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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::Mutex;

use super::repo::ChatMessage;

#[derive(Debug, Clone, Default)]
pub struct FreshnessTracker {
    reads: Arc<Mutex<HashMap<(String, PathBuf), SystemTime>>>,
    /// Agents we've already bootstrapped from history in this process.
    /// Bootstrapping is a one-time scan per process lifetime — a second
    /// invocation would only overwrite precise mtimes with current ones.
    bootstrapped: Arc<Mutex<HashSet<String>>>,
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
        let mtime = match std::fs::metadata(path).and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => return,
        };
        self.record_with_mtime(agent_id, path, mtime).await;
    }

    /// Like `record_read` but with a caller-supplied mtime. Used by
    /// `bootstrap_from_history` so the recorded baseline can be the
    /// historical mtime parsed from a stored tool-result header rather than
    /// the current one — that way external edits during ProcessFox downtime
    /// are still detectable on next chat turn.
    async fn record_with_mtime(&self, agent_id: &str, path: &Path, mtime: SystemTime) {
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => return,
        };
        let mut reads = self.reads.lock().await;
        reads.insert((agent_id.to_string(), canonical), mtime);
    }

    /// Walk persisted chat history and reconstruct the read map for this
    /// agent. Idempotent per process: subsequent calls for the same
    /// `agent_id` are no-ops, so it's safe to call at the top of every
    /// chat run.
    ///
    /// For each successful content-read tool call found, we try to parse
    /// the original mtime out of the stored tool-result header
    /// (`--- foo.md (1006 bytes, modified 2026-05-02T10:31:00Z) ---`). If
    /// the header lacks the timestamp (older logs from before this format
    /// landed) we fall back to the file's *current* mtime — that captures
    /// future external edits but misses anything that happened during
    /// downtime.
    pub async fn bootstrap_from_history(
        &self,
        agent_id: &str,
        history: &[ChatMessage],
        agent_folder: &Path,
    ) {
        {
            let mut bootstrapped = self.bootstrapped.lock().await;
            if !bootstrapped.insert(agent_id.to_string()) {
                return;
            }
        }

        // Index tool_results by tool_use_id so we can pair them with their
        // matching tool_calls without an O(N²) scan.
        let mut results: HashMap<&str, &str> = HashMap::new();
        for msg in history {
            for tr in &msg.tool_results {
                results.insert(tr.tool_use_id.as_str(), tr.content.as_str());
            }
        }

        for msg in history {
            for tc in &msg.tool_calls {
                if !is_content_read_tool(&tc.name) {
                    continue;
                }
                let path_arg = match tc.arguments.get("path").and_then(|v| v.as_str()) {
                    Some(p) if !p.is_empty() => p,
                    _ => continue,
                };
                let abs = agent_folder.join(path_arg);
                let parsed_mtime = results
                    .get(tc.id.as_str())
                    .and_then(|c| parse_mtime_from_header(c));
                match parsed_mtime {
                    Some(t) => self.record_with_mtime(agent_id, &abs, t).await,
                    None => self.record_read(agent_id, &abs).await,
                }
            }
        }
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

/// Tools whose successful execution loads file contents into the LLM's
/// view. Listed explicitly so non-content reads (`list_folder`,
/// `grep_in_files`, `read_skill`) don't trigger spurious staleness
/// warnings — those don't establish a "snapshot of file X".
pub fn is_content_read_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file" | "read_docx" | "read_pdf" | "read_xlsx_range"
    )
}

/// Parse `modified <RFC3339>` out of a tool-result header line. Format is
/// fixed: `... modified YYYY-MM-DDTHH:MM:SSZ ...`. Returns `None` for old
/// logs that predate the mtime addition.
fn parse_mtime_from_header(content: &str) -> Option<SystemTime> {
    let line = content.lines().next()?;
    let idx = line.find("modified ")?;
    let ts_start = idx + "modified ".len();
    let ts_str = line.get(ts_start..ts_start + 20)?;
    if !ts_str.ends_with('Z') {
        return None;
    }
    let dt = chrono::DateTime::parse_from_rfc3339(ts_str).ok()?;
    Some(dt.into())
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

    #[test]
    fn parse_mtime_from_header_with_timestamp() {
        let header = "--- foo.md (1006 bytes, modified 2026-05-02T10:31:00Z) ---";
        let parsed = parse_mtime_from_header(header).expect("must parse");
        let dt: chrono::DateTime<chrono::Utc> = parsed.into();
        assert_eq!(
            dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "2026-05-02T10:31:00Z"
        );
    }

    #[test]
    fn parse_mtime_from_xlsx_style_header() {
        let header = "--- data.xlsx · sheet='Sheet1' · A1:L25 · modified 2026-05-02T10:31:00Z ---";
        assert!(parse_mtime_from_header(header).is_some());
    }

    #[test]
    fn parse_mtime_returns_none_for_old_header_without_timestamp() {
        let header = "--- foo.md (1006 bytes) ---";
        assert!(parse_mtime_from_header(header).is_none());
    }

    #[test]
    fn parse_mtime_returns_none_for_garbage() {
        assert!(parse_mtime_from_header("modified not-a-timestamp").is_none());
        assert!(parse_mtime_from_header("").is_none());
    }

    fn make_assistant_with_call(id: &str, name: &str, path: &str) -> super::ChatMessage {
        super::ChatMessage {
            id: format!("msg_{id}"),
            role: super::super::repo::MessageRole::Assistant,
            content: String::new(),
            created_at: "2026-05-02T10:00:00Z".to_string(),
            tool_calls: vec![crate::core::llm::ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments: serde_json::json!({ "path": path }),
            }],
            tool_results: Vec::new(),
            reasoning: None,
        }
    }

    fn make_tool_result(id: &str, content: &str) -> super::ChatMessage {
        super::ChatMessage {
            id: format!("res_{id}"),
            role: super::super::repo::MessageRole::Tool,
            content: String::new(),
            created_at: "2026-05-02T10:00:01Z".to_string(),
            tool_calls: Vec::new(),
            tool_results: vec![crate::core::llm::ToolResult {
                tool_use_id: id.to_string(),
                content: content.to_string(),
                is_error: false,
            }],
            reasoning: None,
        }
    }

    #[tokio::test]
    async fn bootstrap_uses_mtime_from_result_header() {
        let dir = Tmp::new("bootstrap_with_mtime");
        let f = dir.path().join("plan.md");
        write(&f, "current content");

        // Synthetic history: assistant called read_file, result had a stored
        // mtime in its header that is *older* than the file's current mtime
        // → the file should be flagged as stale right after bootstrap.
        let header = "--- plan.md (15 bytes, modified 2020-01-01T00:00:00Z) ---\nold content";
        let history = vec![
            make_assistant_with_call("call_1", "read_file", "plan.md"),
            make_tool_result("call_1", header),
        ];

        let t = FreshnessTracker::new();
        t.bootstrap_from_history("agent1", &history, dir.path())
            .await;

        let stale = t.stale_paths("agent1").await;
        assert_eq!(stale.len(), 1, "file modified since 2020 must be stale now");
        assert_eq!(stale[0].reason, StaleReason::Modified);
    }

    #[tokio::test]
    async fn bootstrap_falls_back_to_current_mtime_for_legacy_header() {
        let dir = Tmp::new("bootstrap_legacy");
        let f = dir.path().join("plan.md");
        write(&f, "current content");

        // Legacy header without 'modified' field — bootstrap records *now*
        // as baseline. File hasn't been touched since → not stale.
        let history = vec![
            make_assistant_with_call("call_1", "read_file", "plan.md"),
            make_tool_result("call_1", "--- plan.md (15 bytes) ---\nold content"),
        ];

        let t = FreshnessTracker::new();
        t.bootstrap_from_history("agent1", &history, dir.path())
            .await;

        assert!(t.stale_paths("agent1").await.is_empty());
    }

    #[tokio::test]
    async fn bootstrap_is_idempotent_per_agent() {
        let dir = Tmp::new("bootstrap_idempotent");
        let f = dir.path().join("plan.md");
        write(&f, "v1");

        // First bootstrap: precise old mtime → file currently exists, so
        // recording the legacy mtime makes "now > old" → stale.
        let header = "--- plan.md (2 bytes, modified 2020-01-01T00:00:00Z) ---\nv1";
        let history = vec![
            make_assistant_with_call("call_1", "read_file", "plan.md"),
            make_tool_result("call_1", header),
        ];

        let t = FreshnessTracker::new();
        t.bootstrap_from_history("agent1", &history, dir.path())
            .await;
        assert_eq!(t.stale_paths("agent1").await.len(), 1);

        // Second bootstrap with NO history at all: must NOT overwrite the
        // recorded baseline (idempotent guard kicks in). File should still
        // be flagged as stale.
        t.bootstrap_from_history("agent1", &[], dir.path()).await;
        assert_eq!(
            t.stale_paths("agent1").await.len(),
            1,
            "second bootstrap call must not blow away the precise baseline"
        );
    }

    #[tokio::test]
    async fn bootstrap_skips_non_content_read_tools() {
        let dir = Tmp::new("bootstrap_skip");
        let f = dir.path().join("plan.md");
        write(&f, "v1");

        // History contains a list_folder call (not a content read) — should
        // not record any file in the tracker.
        let history = vec![
            super::ChatMessage {
                id: "msg1".to_string(),
                role: super::super::repo::MessageRole::Assistant,
                content: String::new(),
                created_at: "2026-05-02T10:00:00Z".to_string(),
                tool_calls: vec![crate::core::llm::ToolCall {
                    id: "call_1".to_string(),
                    name: "list_folder".to_string(),
                    arguments: serde_json::json!({ "path": "" }),
                }],
                tool_results: Vec::new(),
                reasoning: None,
            },
            make_tool_result("call_1", "files listed"),
        ];

        let t = FreshnessTracker::new();
        t.bootstrap_from_history("agent1", &history, dir.path())
            .await;
        assert!(t.stale_paths("agent1").await.is_empty());
    }
}
