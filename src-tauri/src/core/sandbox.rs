use std::path::{Path, PathBuf};

use super::error::{CoreError, CoreResult};

/// Ensure `requested` lies inside `agent_folder`. Returns the canonical,
/// absolute path on success.
///
/// Resolves symlinks via `canonicalize` to prevent escape via symlinks
/// pointing outside the agent folder.
pub fn ensure_in_agent_folder(agent_folder: &Path, requested: &Path) -> CoreResult<PathBuf> {
    let absolute = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        agent_folder.join(requested)
    };

    let canonical_requested = absolute
        .canonicalize()
        .map_err(|e| CoreError::PathInvalid(e.to_string()))?;
    let canonical_root = agent_folder
        .canonicalize()
        .map_err(|e| CoreError::PathInvalid(e.to_string()))?;

    if !canonical_requested.starts_with(&canonical_root) {
        return Err(CoreError::PathOutsideAgentFolder);
    }

    Ok(canonical_requested)
}

/// Resolve `requested` inside `agent_folder` for a HITL preview read: same
/// boundary guarantee as `ensure_in_agent_folder`, but never propagates an
/// error — any path that doesn't resolve (missing, outside the folder, or
/// otherwise unreadable) simply yields `None`, so a preview never falls back
/// to reading raw, unsandboxed content. `requires_approval()` implementations
/// must go through this instead of joining `agent_folder` with the
/// LLM-supplied path directly — that raw join is exactly what let a
/// model-supplied absolute or `../`-path leak file content into the approval
/// card before the user ever decided anything.
pub fn resolve_for_preview(agent_folder: &Path, requested: &Path) -> Option<PathBuf> {
    ensure_in_agent_folder(agent_folder, requested).ok()
}

/// Like `ensure_in_agent_folder`, but for tools whose target file (and
/// possibly its parent directories) may not exist yet. Boundary checks run
/// BEFORE any filesystem mutation:
///
/// 1. Resolve `.`/`..` purely lexically (no disk access), so a crafted
///    `../../etc/...` argument can never reach a `create_dir_all` call.
/// 2. Reject outright if the lexically-resolved path doesn't start inside
///    the canonical agent folder.
/// 3. Walk up to the deepest *existing* ancestor and canonicalize it — this
///    resolves any symlink planted inside the agent folder (e.g. a
///    subfolder that points outside) and re-checks the boundary against the
///    resolved target.
/// 4. Only once both checks pass does it `create_dir_all` the missing
///    parent directories.
///
/// The previous per-tool copies of this helper created directories before
/// checking the boundary, which meant a malicious path could trigger a real
/// out-of-sandbox `mkdir` even though the final write was still correctly
/// rejected afterward.
pub fn ensure_inside_sandbox(agent_folder: &Path, requested: &Path) -> CoreResult<PathBuf> {
    let canonical_root = agent_folder
        .canonicalize()
        .map_err(|e| CoreError::PathInvalid(e.to_string()))?;

    let raw = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        canonical_root.join(requested)
    };
    let normalized = lexically_normalize(&raw);
    if !normalized.starts_with(&canonical_root) {
        return Err(CoreError::PathOutsideAgentFolder);
    }

    // Find the deepest existing ancestor of the (already dot-free) target
    // and canonicalize it — this is what catches an existing symlink
    // somewhere inside the agent folder redirecting outside, without ever
    // touching disk for the not-yet-existing tail.
    let mut existing_ancestor = normalized.clone();
    while !existing_ancestor.exists() {
        existing_ancestor = existing_ancestor
            .parent()
            .ok_or_else(|| CoreError::PathInvalid(requested.display().to_string()))?
            .to_path_buf();
    }
    let canonical_existing = existing_ancestor
        .canonicalize()
        .map_err(|e| CoreError::PathInvalid(e.to_string()))?;
    if !canonical_existing.starts_with(&canonical_root) {
        return Err(CoreError::PathOutsideAgentFolder);
    }

    let parent = normalized
        .parent()
        .ok_or_else(|| CoreError::PathInvalid(requested.display().to_string()))?;
    std::fs::create_dir_all(parent).map_err(|e| CoreError::PathInvalid(e.to_string()))?;

    let canonical_parent = parent
        .canonicalize()
        .map_err(|e| CoreError::PathInvalid(e.to_string()))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(CoreError::PathOutsideAgentFolder);
    }
    let filename = normalized
        .file_name()
        .ok_or_else(|| CoreError::PathInvalid(requested.display().to_string()))?;
    Ok(canonical_parent.join(filename))
}

/// Resolve `.`/`..` components without touching the filesystem. `PathBuf::pop`
/// already refuses to climb past a root/prefix component (it's a no-op once
/// nothing is left to pop), so `..` past the top of an absolute path simply
/// stays put — it does not error here. The caller's `starts_with` check
/// against the canonical agent folder is what actually rejects an escape
/// attempt; this function's only job is to make sure no `..` survives to
/// reach a `create_dir_all` call.
fn lexically_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "processfox_sandbox_{prefix}_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn relative_path_inside_resolves() {
        let root = tmp_dir("relative");
        let inner = root.join("notes.md");
        fs::write(&inner, "hi").unwrap();

        let resolved = ensure_in_agent_folder(&root, Path::new("notes.md")).unwrap();
        assert_eq!(resolved, inner.canonicalize().unwrap());
    }

    #[test]
    fn absolute_path_outside_is_rejected() {
        let root = tmp_dir("absolute");
        let outside = std::env::temp_dir().join("processfox_sandbox_outside_target");
        fs::write(&outside, "nope").unwrap();

        let err = ensure_in_agent_folder(&root, &outside).unwrap_err();
        assert!(matches!(err, CoreError::PathOutsideAgentFolder));
    }

    #[test]
    fn parent_traversal_is_rejected() {
        let root = tmp_dir("traversal");
        let sibling = root
            .parent()
            .unwrap()
            .join(format!("processfox_sandbox_sibling_{}", std::process::id()));
        fs::create_dir_all(&sibling).unwrap();
        let leak = sibling.join("leak.txt");
        fs::write(&leak, "leak").unwrap();

        let relative = Path::new("../")
            .join(sibling.file_name().unwrap())
            .join("leak.txt");
        let err = ensure_in_agent_folder(&root, &relative).unwrap_err();
        assert!(matches!(err, CoreError::PathOutsideAgentFolder));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = tmp_dir("symlink");
        let outside = std::env::temp_dir().join(format!(
            "processfox_sandbox_sym_target_{}",
            std::process::id()
        ));
        fs::create_dir_all(&outside).unwrap();
        let target = outside.join("secret.txt");
        fs::write(&target, "secret").unwrap();

        let link = root.join("link-out");
        symlink(&outside, &link).unwrap();

        // Accessing through the symlink should be refused — canonicalize
        // resolves it to the outside target.
        let err = ensure_in_agent_folder(&root, Path::new("link-out/secret.txt")).unwrap_err();
        assert!(matches!(err, CoreError::PathOutsideAgentFolder));
    }

    #[test]
    fn nonexistent_path_fails_cleanly() {
        let root = tmp_dir("missing");
        let err = ensure_in_agent_folder(&root, Path::new("does-not-exist")).unwrap_err();
        assert!(matches!(err, CoreError::PathInvalid(_)));
    }

    #[test]
    fn preview_resolve_returns_some_for_existing_inside_file() {
        let root = tmp_dir("preview_ok");
        fs::write(root.join("notes.md"), "hi").unwrap();
        let resolved = resolve_for_preview(&root, Path::new("notes.md"));
        assert!(resolved.is_some());
    }

    #[test]
    fn preview_resolve_returns_none_for_missing_file() {
        let root = tmp_dir("preview_missing");
        assert!(resolve_for_preview(&root, Path::new("does-not-exist.md")).is_none());
    }

    #[test]
    fn preview_resolve_returns_none_for_outside_absolute_path() {
        let root = tmp_dir("preview_outside");
        let outside = std::env::temp_dir().join("processfox_sandbox_preview_outside_target");
        fs::write(&outside, "secret").unwrap();
        assert!(resolve_for_preview(&root, &outside).is_none());
    }

    #[test]
    fn inside_sandbox_creates_missing_nested_dirs() {
        let root = tmp_dir("inside_nested");
        let resolved = ensure_inside_sandbox(&root, Path::new("reports/2026/april.docx")).unwrap();
        assert!(resolved.parent().unwrap().is_dir());
        assert!(resolved.starts_with(root.canonicalize().unwrap()));
    }

    #[test]
    fn inside_sandbox_rejects_absolute_outside_without_mkdir() {
        let root = tmp_dir("inside_absolute_outside");
        let outside = std::env::temp_dir().join(format!(
            "processfox_sandbox_inside_outside_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&outside);
        let target = outside.join("evil.docx");

        let err = ensure_inside_sandbox(&root, &target).unwrap_err();
        assert!(matches!(err, CoreError::PathOutsideAgentFolder));
        // The critical assertion: rejection must not have created the
        // out-of-sandbox directory as a side effect.
        assert!(!outside.exists());
    }

    #[test]
    fn inside_sandbox_rejects_parent_traversal_without_mkdir() {
        let root = tmp_dir("inside_traversal");
        let leak_marker = root.parent().unwrap().join(format!(
            "processfox_sandbox_leak_marker_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&leak_marker);

        let relative = Path::new("../")
            .join(leak_marker.file_name().unwrap())
            .join("evil.docx");
        let err = ensure_inside_sandbox(&root, &relative).unwrap_err();
        assert!(matches!(err, CoreError::PathOutsideAgentFolder));
        assert!(!leak_marker.exists());
    }

    #[cfg(unix)]
    #[test]
    fn inside_sandbox_rejects_existing_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = tmp_dir("inside_symlink");
        let outside = std::env::temp_dir().join(format!(
            "processfox_sandbox_inside_sym_target_{}",
            std::process::id()
        ));
        fs::create_dir_all(&outside).unwrap();

        let link = root.join("escape-link");
        symlink(&outside, &link).unwrap();

        let err = ensure_inside_sandbox(&root, Path::new("escape-link/evil.docx")).unwrap_err();
        assert!(matches!(err, CoreError::PathOutsideAgentFolder));
        // Nothing should have been written through the symlink target.
        assert!(!outside.join("evil.docx").exists());
    }
}
