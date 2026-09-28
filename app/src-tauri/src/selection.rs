use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use uuid::Uuid;

use crate::model::{FileStatus, PublicSelectedFile};

pub struct SelectedFile {
    pub id: String,
    /// Path as selected by the user. Rust-only: never crosses the IPC boundary.
    pub path: PathBuf,
    /// Canonicalised path; the deduplication key.
    pub canonical: PathBuf,
    pub display_name: String,
    pub extension: Option<String>,
    pub relative_path: Option<PathBuf>,
    pub size: u64,
    #[allow(dead_code)] // Task 4: explicit symlink classification; read via test-gated is_symlink
    pub symlink: bool,
    pub status: FileStatus,
}

#[derive(Default)]
pub struct Registry {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    order: Vec<String>,
    by_id: HashMap<String, SelectedFile>,
    by_canonical: HashMap<PathBuf, String>,
}

#[derive(Default, Debug)]
pub struct AddOutcome {
    pub added: Vec<String>,
    pub duplicates: Vec<String>,
    pub skipped: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct SelectionSnapshot {
    pub path: PathBuf,
    pub display_name: String,
    pub relative_path: Option<PathBuf>,
}

/// Enumerate regular files under `root` deterministically (sorted).
///
/// SECURITY INVARIANT: never follows symlinks — a symlinked entry (file or
/// directory) is skipped entirely, so a hostile folder tree cannot redirect
/// enumeration outside `root` (HANDOFF §16, plan Task 4).
pub fn enumerate_folder(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    enumerate_into(root, &mut out);
    out.sort();
    out
}

fn enumerate_into(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let path = entry.path();
        if ft.is_dir() {
            enumerate_into(&path, out);
        } else if ft.is_file() {
            out.push(path);
        }
    }
}

fn classify(path: &Path, root: Option<&Path>) -> Result<SelectedFile, String> {
    let sym_md = fs::symlink_metadata(path).map_err(|e| format!("cannot read: {e}"))?;
    let symlink = sym_md.file_type().is_symlink();
    if sym_md.file_type().is_dir() {
        return Err("is a directory".to_string());
    }
    let md = fs::metadata(path).map_err(|e| format!("target unreadable: {e}"))?;
    if !md.is_file() {
        return Err("not a regular file".to_string());
    }
    let canonical = fs::canonicalize(path).map_err(|e| format!("cannot resolve: {e}"))?;
    let display_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| "invalid file name".to_string())?;
    let extension = path.extension().map(|e| e.to_string_lossy().into_owned());
    let relative_path = root
        .and_then(|r| path.strip_prefix(r).ok())
        .map(PathBuf::from);

    Ok(SelectedFile {
        id: Uuid::new_v4().to_string(),
        path: path.to_path_buf(),
        canonical,
        display_name,
        extension,
        relative_path,
        size: md.len(),
        symlink,
        status: FileStatus::Ready,
    })
}

impl Registry {
    /// Register user-approved file paths. `root` (folder batches) enables
    /// relative-path recording against the canonicalised batch root.
    /// Duplicate paths (by canonical resolution) are reported, never re-added.
    pub fn add_paths(&self, paths: &[PathBuf], root: Option<&Path>) -> AddOutcome {
        let mut outcome = AddOutcome::default();
        let mut inner = self.inner.lock().expect("registry poisoned");
        for path in paths {
            let display = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            match classify(path, root) {
                Err(reason) => outcome.skipped.push((display, reason)),
                Ok(mut entry) => {
                    if let Some(existing) = inner.by_canonical.get(&entry.canonical) {
                        outcome.duplicates.push(existing.clone());
                        continue;
                    }
                    if entry.relative_path.is_none()
                        && let Some(r) = root
                    {
                        entry.relative_path =
                            entry.canonical.strip_prefix(r).ok().map(PathBuf::from);
                    }
                    let id = entry.id.clone();
                    inner
                        .by_canonical
                        .insert(entry.canonical.clone(), id.clone());
                    inner.by_id.insert(id.clone(), entry);
                    inner.order.push(id.clone());
                    outcome.added.push(id);
                }
            }
        }
        outcome
    }

    /// Rust-internal resolution: opaque id → approved absolute path.
    /// Unknown ids resolve to nothing; the frontend can never express a path.
    #[cfg(test)]
    pub fn resolve(&self, id: &str) -> Option<PathBuf> {
        self.inner
            .lock()
            .expect("registry poisoned")
            .by_id
            .get(id)
            .map(|e| e.path.clone())
    }

    pub fn snapshot(&self, id: &str) -> Option<SelectionSnapshot> {
        self.inner
            .lock()
            .expect("registry poisoned")
            .by_id
            .get(id)
            .map(|e| SelectionSnapshot {
                path: e.path.clone(),
                display_name: e.display_name.clone(),
                relative_path: e.relative_path.clone(),
            })
    }

    #[cfg(test)]
    pub fn is_symlink(&self, id: &str) -> Option<bool> {
        self.inner
            .lock()
            .expect("registry poisoned")
            .by_id
            .get(id)
            .map(|e| e.symlink)
    }

    pub fn set_status(&self, id: &str, status: FileStatus) -> bool {
        let mut inner = self.inner.lock().expect("registry poisoned");
        match inner.by_id.get_mut(id) {
            Some(e) => {
                e.status = status;
                true
            }
            None => false,
        }
    }

    pub fn remove(&self, ids: &[String]) -> usize {
        let mut inner = self.inner.lock().expect("registry poisoned");
        let mut removed = 0usize;
        for id in ids {
            if let Some(entry) = inner.by_id.remove(id) {
                inner.by_canonical.remove(&entry.canonical);
                inner.order.retain(|i| i != id);
                removed += 1;
            }
        }
        removed
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.lock().expect("registry poisoned").order.len()
    }

    pub fn public_list(&self) -> Vec<PublicSelectedFile> {
        let inner = self.inner.lock().expect("registry poisoned");
        inner
            .order
            .iter()
            .filter_map(|id| inner.by_id.get(id))
            .map(|e| PublicSelectedFile {
                id: e.id.clone(),
                display_name: e.display_name.clone(),
                extension: e.extension.clone(),
                relative_path: e
                    .relative_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                size: e.size,
                status: e.status,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!(
            "mat2wrap-test-{}-{}-{}",
            tag,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_file(p: &Path, bytes: &[u8]) {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, bytes).unwrap();
    }

    #[test]
    fn unknown_id_cannot_resolve_to_a_path() {
        let reg = Registry::default();
        assert!(reg.resolve("nonexistent").is_none());

        let dir = tempdir("unknown");
        let f = dir.join("a.jpg");
        write_file(&f, b"x");
        reg.add_paths(std::slice::from_ref(&f), None);
        assert!(reg.resolve("nonexistent").is_none());
        assert!(reg.resolve("").is_none());
        assert!(reg.resolve("../etc/passwd").is_none());
        assert!(reg.resolve(&f.to_string_lossy()).is_none());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn registry_owns_only_approved_paths() {
        let dir = tempdir("approved");
        let f = dir.join("a.jpg");
        write_file(&f, b"x");
        let reg = Registry::default();
        let outcome = reg.add_paths(std::slice::from_ref(&f), None);
        assert_eq!(outcome.added.len(), 1);
        let id = outcome.added[0].clone();
        assert_eq!(reg.resolve(&id).unwrap(), f);
        assert_eq!(reg.len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn duplicate_addition_is_deterministic() {
        let dir = tempdir("dup");
        let f = dir.join("a.jpg");
        write_file(&f, b"data");
        let reg = Registry::default();

        let first = reg.add_paths(std::slice::from_ref(&f), None);
        assert_eq!(first.added.len(), 1);
        assert!(first.duplicates.is_empty());

        let second = reg.add_paths(std::slice::from_ref(&f), None);
        assert!(second.added.is_empty());
        assert_eq!(second.duplicates, first.added);
        assert_eq!(reg.len(), 1);

        // same target reached through a symlink also dedupes (canonical key)
        let link = dir.join("link.jpg");
        std::os::unix::fs::symlink(&f, &link).unwrap();
        let third = reg.add_paths(std::slice::from_ref(&link), None);
        assert!(third.added.is_empty());
        assert_eq!(third.duplicates, first.added);
        assert_eq!(reg.len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn html_like_display_names_remain_data() {
        let dir = tempdir("html");
        let hostile = dir.join("<img src=x onerror=alert(1)>.jpg");
        write_file(&hostile, b"x");
        let reg = Registry::default();
        reg.add_paths(std::slice::from_ref(&hostile), None);
        let list = reg.public_list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].display_name, "<img src=x onerror=alert(1)>.jpg");
        assert_eq!(list[0].extension.as_deref(), Some("jpg"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn symlink_classification_is_explicit() {
        let dir = tempdir("symlink");
        let target = dir.join("target.jpg");
        write_file(&target, b"x");
        let link = dir.join("link.jpg");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let reg = Registry::default();
        let via_link = reg.add_paths(std::slice::from_ref(&link), None);
        assert_eq!(via_link.added.len(), 1);
        assert_eq!(reg.is_symlink(&via_link.added[0]), Some(true));

        // the same content reached directly dedupes against the link entry
        let direct = reg.add_paths(std::slice::from_ref(&target), None);
        assert!(direct.added.is_empty());
        assert_eq!(direct.duplicates, via_link.added);

        // a regular file is classified as non-symlink
        let plain = dir.join("plain.txt");
        write_file(&plain, b"y");
        let p = reg.add_paths(&[plain], None);
        assert_eq!(reg.is_symlink(&p.added[0]), Some(false));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn broken_symlink_is_skipped_with_reason() {
        let dir = tempdir("broken");
        let link = dir.join("broken.jpg");
        std::os::unix::fs::symlink(dir.join("nowhere.jpg"), &link).unwrap();
        let reg = Registry::default();
        let outcome = reg.add_paths(std::slice::from_ref(&link), None);
        assert!(outcome.added.is_empty());
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].0, "broken.jpg");
        assert_eq!(reg.len(), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn enumeration_skips_symlinks_and_records_relative_paths() {
        let dir = tempdir("enum");
        let root = dir.join("batch");
        write_file(&root.join("a.txt"), b"1");
        write_file(&root.join("sub/b.png"), b"22");
        write_file(&root.join("sub/deep/c.pdf"), b"333");

        // symlinked file inside the tree
        std::os::unix::fs::symlink(root.join("a.txt"), root.join("link-to-a.txt")).unwrap();
        // symlinked directory pointing OUTSIDE the root — must not be followed
        let outside = dir.join("outside");
        write_file(&outside.join("secret.txt"), b"s");
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();

        let files = enumerate_folder(&root);
        let names: Vec<String> = files
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(
            names,
            vec![
                "a.txt".to_string(),
                "sub/b.png".to_string(),
                "sub/deep/c.pdf".to_string()
            ]
        );
        assert!(
            !names
                .iter()
                .any(|n| n.contains("escape") || n.contains("link-to-a") || n.contains("secret"))
        );

        let reg = Registry::default();
        let canonical_root = fs::canonicalize(&root).unwrap();
        let outcome = reg.add_paths(&files, Some(&canonical_root));
        assert_eq!(outcome.added.len(), 3);
        let rels: Vec<Option<String>> = reg
            .public_list()
            .into_iter()
            .map(|f| f.relative_path)
            .collect();
        assert!(rels.contains(&Some("a.txt".to_string())));
        assert!(rels.contains(&Some("sub/b.png".to_string())));
        assert!(rels.contains(&Some("sub/deep/c.pdf".to_string())));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn remove_is_deterministic_with_unknown_ids() {
        let dir = tempdir("remove");
        let a = dir.join("a.jpg");
        let b = dir.join("b.jpg");
        write_file(&a, b"1");
        write_file(&b, b"2");
        let reg = Registry::default();
        let outcome = reg.add_paths(&[a, b], None);
        assert_eq!(reg.len(), 2);

        let removed = reg.remove(&[outcome.added[0].clone(), "ghost-id".to_string()]);
        assert_eq!(removed, 1);
        assert_eq!(reg.len(), 1);
        assert!(reg.resolve(&outcome.added[0]).is_none());
        assert!(reg.resolve(&outcome.added[1]).is_some());

        // re-adding a removed file works (canonical key was released)
        let again = reg.add_paths(&[dir.join("a.jpg")], None);
        assert_eq!(again.added.len(), 1);
        assert_eq!(reg.len(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }
}
