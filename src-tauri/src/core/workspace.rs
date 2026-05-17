//! Workspace orientation for the system prompt.
//!
//! Renders a bounded, dated overview of the agent folder so the model knows
//! *that* and *which* other files exist before it answers. Without this, a
//! query like "what happened in the project in April?" gets answered from
//! whatever single file already sits in context (e.g. a referenced
//! CLAUDE.md) because the model has no inventory to reason about.
//!
//! Built once per user turn from `compose_system_prompt` (not per ReAct
//! iteration), so the bounded filesystem walk is negligible and the snapshot
//! stays fresh each turn — complementing the `FreshnessTracker`.
//!
//! Best-effort: any IO failure (unreadable subfolder, missing metadata)
//! is skipped rather than propagated, so prompt composition never fails on
//! a filesystem hiccup.

use std::path::Path;

/// Hard cap on emitted entries. Tighter than `list_folder`'s 200 because
/// this is orientation, not enumeration: ~25 tokens/line → worst case ~1k
/// tokens per turn. Tunable.
const WORKSPACE_MAX_ENTRIES: usize = 40;

/// Levels of content shown: root entries plus the immediate children of
/// root subfolders. Deeper folders appear as a header line without contents.
const MAX_DEPTH: usize = 2;

/// Same junk filter as `list_folder` — surfacing these would only confuse.
const DENYLIST: &[&str] = &[".DS_Store", "Thumbs.db", ".Spotlight-V100"];

struct Walk {
    out: String,
    emitted: usize,
    capped: bool,
}

/// Build a compact, indented tree of `root`. Returns `None` when the folder
/// can't be resolved, isn't a directory, or has no listable entries — the
/// caller then omits the `## Workspace` block entirely instead of emitting
/// an empty header.
pub fn build_workspace_tree(root: &Path) -> Option<String> {
    let canon = root.canonicalize().ok()?;
    if !canon.is_dir() {
        return None;
    }
    let mut walk = Walk {
        out: String::new(),
        emitted: 0,
        capped: false,
    };
    descend(&canon, 0, &mut walk);
    if walk.emitted == 0 {
        return None;
    }
    if walk.capped {
        // No exact remaining count: tallying it would require a second full
        // traversal, and the model only needs to know the list is partial.
        walk.out
            .push_str("… weitere Einträge ausgelassen — nutze list_folder/grep_in_files\n");
    }
    Some(walk.out)
}

fn descend(dir: &Path, depth: usize, walk: &mut Walk) {
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return, // unreadable subfolder: skip, don't fail the prompt
    };

    // Collect (name, is_dir) up front so we can sort dirs-first, then alpha.
    let mut entries: Vec<(String, bool, std::fs::Metadata)> = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if DENYLIST.contains(&name.as_str()) {
            continue;
        }
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        // Skip symlinks entirely: keeps the walk escape-safe and cycle-free
        // without following links out of the agent folder.
        if ft.is_symlink() {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        entries.push((name, ft.is_dir(), meta));
    }

    entries.sort_by(|a, b| match (a.1, b.1) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.0.to_lowercase().cmp(&b.0.to_lowercase()),
    });

    let indent = "  ".repeat(depth);
    for (name, is_dir, meta) in entries {
        if walk.emitted >= WORKSPACE_MAX_ENTRIES {
            walk.capped = true;
            return;
        }
        if is_dir {
            walk.out.push_str(&format!("{indent}📁 {name}/\n"));
            walk.emitted += 1;
            // Recurse only while another content level is allowed; at the
            // deepest level the folder is shown as a header without contents.
            if depth + 1 < MAX_DEPTH {
                descend(&dir.join(&name), depth + 1, walk);
                if walk.capped {
                    return;
                }
            }
        } else {
            walk.out.push_str(&format!(
                "{indent}📄 {name}  ({} · {})\n",
                human_bytes(meta.len()),
                fmt_mtime(&meta),
            ));
            walk.emitted += 1;
        }
    }
}

/// Local date, day resolution. Deliberately *not* the RFC-3339 UTC format
/// the read-tool headers use (no parser depends on this), and seconds add
/// tokens without helping orientation.
fn fmt_mtime(meta: &std::fs::Metadata) -> String {
    meta.modified()
        .ok()
        .map(|t| {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.format("%Y-%m-%d").to_string()
        })
        .unwrap_or_else(|| "?".to_string())
}

/// Mirrors `list_folder`'s thresholds for a consistent feel. A shared
/// primitive across both call sites is a deliberate follow-up, not part of
/// this change, to keep the diff focused.
fn human_bytes(b: u64) -> String {
    const K: u64 = 1024;
    if b < K {
        format!("{b} B")
    } else if b < K * K {
        format!("{:.1} KB", b as f64 / K as f64)
    } else if b < K * K * K {
        format!("{:.1} MB", b as f64 / (K * K) as f64)
    } else {
        format!("{:.2} GB", b as f64 / (K * K * K) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Disposable temp dir, cleaned on drop. Mirrors the pattern in
    /// `core/chat/freshness.rs::tests` so we don't pull in an extra crate.
    struct Tmp(PathBuf);

    impl Tmp {
        fn new(prefix: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "processfox_workspace_{prefix}_{}_{}",
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

    #[test]
    fn missing_folder_is_none() {
        let missing = std::env::temp_dir().join("processfox_workspace_does_not_exist_xyz");
        assert!(build_workspace_tree(&missing).is_none());
    }

    #[test]
    fn empty_folder_is_none() {
        let dir = Tmp::new("empty");
        assert!(build_workspace_tree(dir.path()).is_none());
    }

    #[test]
    fn folder_with_only_denylisted_is_none() {
        let dir = Tmp::new("denyonly");
        std::fs::write(dir.path().join(".DS_Store"), "junk").unwrap();
        assert!(build_workspace_tree(dir.path()).is_none());
    }

    #[test]
    fn flat_folder_lists_files_with_size() {
        let dir = Tmp::new("flat");
        std::fs::write(dir.path().join("notes.md"), "hello").unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "ctx").unwrap();
        let tree = build_workspace_tree(dir.path()).unwrap();
        assert!(tree.contains("📄 notes.md  ("), "got:\n{tree}");
        assert!(tree.contains("📄 CLAUDE.md  ("), "got:\n{tree}");
        // Alphabetical, case-insensitive: CLAUDE.md before notes.md.
        let a = tree.find("CLAUDE.md").unwrap();
        let b = tree.find("notes.md").unwrap();
        assert!(a < b, "expected alpha order, got:\n{tree}");
    }

    #[test]
    fn dirs_listed_before_files() {
        let dir = Tmp::new("order");
        std::fs::create_dir(dir.path().join("reports")).unwrap();
        std::fs::write(dir.path().join("aaa.md"), "x").unwrap();
        let tree = build_workspace_tree(dir.path()).unwrap();
        let d = tree.find("📁 reports/").unwrap();
        let f = tree.find("📄 aaa.md").unwrap();
        assert!(d < f, "dirs should sort before files, got:\n{tree}");
    }

    #[test]
    fn depth_cap_shows_level2_dir_header_without_contents() {
        let dir = Tmp::new("depth");
        // root/a/ (level 1) -> a/b/ (level 2 header) -> a/b/deep.md (omitted)
        let a = dir.path().join("a");
        let b = a.join("b");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("shallow.md"), "x").unwrap();
        std::fs::write(b.join("deep.md"), "y").unwrap();
        let tree = build_workspace_tree(dir.path()).unwrap();
        assert!(tree.contains("📁 a/"), "got:\n{tree}");
        assert!(tree.contains("📄 shallow.md"), "got:\n{tree}");
        assert!(tree.contains("📁 b/"), "got:\n{tree}");
        assert!(
            !tree.contains("deep.md"),
            "level-3 content must be omitted, got:\n{tree}"
        );
    }

    #[test]
    fn entry_cap_truncates_with_marker() {
        let dir = Tmp::new("cap");
        for i in 0..(WORKSPACE_MAX_ENTRIES + 10) {
            std::fs::write(dir.path().join(format!("f{i:03}.md")), "x").unwrap();
        }
        let tree = build_workspace_tree(dir.path()).unwrap();
        assert!(
            tree.contains("weitere Einträge ausgelassen"),
            "got:\n{tree}"
        );
        let listed = tree.matches("📄 ").count();
        assert!(
            listed <= WORKSPACE_MAX_ENTRIES,
            "emitted {listed} > cap {WORKSPACE_MAX_ENTRIES}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_dir_is_not_descended() {
        let dir = Tmp::new("symlink");
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("secret.md"), "x").unwrap();
        std::os::unix::fs::symlink(&real, dir.path().join("link")).unwrap();
        std::fs::write(dir.path().join("plain.md"), "y").unwrap();
        let tree = build_workspace_tree(dir.path()).unwrap();
        assert!(tree.contains("📄 plain.md"), "got:\n{tree}");
        assert!(tree.contains("📁 real/"), "got:\n{tree}");
        // The symlink itself is skipped, and nothing is reached through it.
        assert!(
            !tree.contains("link"),
            "symlink should be skipped, got:\n{tree}"
        );
    }
}
