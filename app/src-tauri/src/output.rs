use std::fs;
use std::path::{Component, Path, PathBuf};

use time::macros::format_description;
use time::OffsetDateTime;

pub const OUTPUT_DIR_NAME: &str = "MAT2 Output";

#[derive(Clone, Debug)]
pub enum OutputMode {
    BesideSource,
    Custom(PathBuf),
}

pub fn timestamp_now() -> String {
    let fmt = format_description!("[year]-[month]-[day]_[hour][minute][second]");
    let now = OffsetDateTime::now_local()
        .unwrap_or_else(|_| OffsetDateTime::now_utc());
    now.format(&fmt).expect("timestamp formatting")
}

/// Compute (without creating) the output root for a job:
/// - BesideSource: `<source_parent>/MAT2 Output/<timestamp>/`
/// - Custom:       `<custom_root>/<timestamp>/`
pub fn job_root(mode: &OutputMode, source_parent: &Path, timestamp: &str) -> Result<PathBuf, String> {
    validate_single_components(timestamp, "timestamp")?;
    match mode {
        OutputMode::BesideSource => {
            let parent = fs::canonicalize(source_parent)
                .map_err(|e| format!("source parent unreadable: {e}"))?;
            if !parent.is_dir() {
                return Err("source parent is not a directory".into());
            }
            Ok(parent.join(OUTPUT_DIR_NAME).join(timestamp))
        }
        OutputMode::Custom(root) => {
            let root = fs::canonicalize(root)
                .map_err(|e| format!("custom output root unreadable: {e}"))?;
            if !root.is_dir() {
                return Err("custom output root is not a directory".into());
            }
            Ok(root.join(timestamp))
        }
    }
}

/// Create the output root (and missing parents below an existing base dir),
/// validating every created component is a real directory — never a symlink —
/// and return its canonical path. All later containment checks compare
/// against this canonical root.
pub fn ensure_root(root: &Path) -> Result<PathBuf, String> {
    let mut built = PathBuf::new();
    for component in root.components() {
        built.push(component);
        match fs::symlink_metadata(&built) {
            Ok(md) => {
                let ft = md.file_type();
                if ft.is_symlink() {
                    // SECURITY INVARIANT: refuse to build output paths through
                    // symlinks; a planted link could redirect writes outside
                    // the approved root.
                    return Err(format!("output path component is a symlink: {:?}", built));
                }
                if !ft.is_dir() {
                    return Err(format!("output path component is not a directory: {:?}", built));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&built)
                    .map_err(|e2| format!("cannot create output dir {:?}: {e2}", built))?;
                let md = fs::symlink_metadata(&built)
                    .map_err(|e2| format!("cannot verify created dir {:?}: {e2}", built))?;
                if md.file_type().is_symlink() || !md.file_type().is_dir() {
                    return Err(format!("created component is not a real directory: {:?}", built));
                }
            }
            Err(e) => return Err(format!("cannot inspect {:?}: {e}", built)),
        }
    }
    fs::canonicalize(root).map_err(|e| format!("cannot canonicalize output root: {e}"))
}

/// Plan the final, collision-free destination for one cleaned file.
///
/// - `cleaned_name` must be a single file name component (the `.cleaned`
///   name produced by MAT2 on the staged copy).
/// - `relative_dir` (folder batches) must be a relative path without `..`
///   components; it is recreated under the canonical root.
/// - Existing files are never overwritten: `x.cleaned.jpg` →
///   `x.cleaned-2.jpg` → `x.cleaned-3.jpg` …
/// - The canonicalised result is guaranteed to sit inside `canonical_root`.
pub fn plan_final_path(
    canonical_root: &Path,
    relative_dir: Option<&Path>,
    cleaned_name: &str,
) -> Result<PathBuf, String> {
    if cleaned_name.is_empty() {
        return Err("empty output name".into());
    }
    validate_single_components(cleaned_name, "output name")?;
    if let Some(rel) = relative_dir {
        if rel.is_absolute() {
            return Err("relative_dir must be relative".into());
        }
        for c in rel.components() {
            match c {
                Component::ParentDir => {
                    return Err("relative_dir must not contain '..'".into());
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err("relative_dir must be relative".into());
                }
                Component::CurDir => {}
                Component::Normal(_) => {}
            }
        }
    }

    let base_dir = match relative_dir {
        Some(rel) if rel != Path::new("") && rel != Path::new(".") => {
            let joined = canonical_root.join(rel);
            ensure_root(&joined)?;
            joined
        }
        _ => canonical_root.to_path_buf(),
    };

    // SECURITY INVARIANT: containment re-check after any directory creation —
    // the canonical parent must remain inside the canonical root.
    let base_canonical = fs::canonicalize(&base_dir)
        .map_err(|e| format!("cannot canonicalize output subdir: {e}"))?;
    if !base_canonical.starts_with(canonical_root) {
        return Err(format!(
            "output subdir escapes the approved root: {:?}",
            base_canonical
        ));
    }

    let mut candidate = base_canonical.join(cleaned_name);
    let mut counter = 2u32;
    while path_exists(&candidate) {
        candidate = base_canonical.join(collided_name(cleaned_name, counter));
        counter += 1;
        if counter > 10_000 {
            return Err("collision counter exhausted".into());
        }
    }
    Ok(candidate)
}

fn path_exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

fn collided_name(name: &str, n: u32) -> String {
    let path = Path::new(name);
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| name.to_string());
    match path.extension() {
        Some(ext) => format!("{stem}-{n}.{}", ext.to_string_lossy()),
        None => format!("{stem}-{n}"),
    }
}

fn validate_single_components(name: &str, what: &str) -> Result<(), String> {
    let p = Path::new(name);
    if p.is_absolute() {
        return Err(format!("{what} must not be absolute"));
    }
    let mut comps = p.components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(format!("{what} must be a single path component: {name:?}")),
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
        let p = std::env::temp_dir().join(format!("mat2out-{}-{}-{}", tag, std::process::id(), nanos));
        fs::create_dir_all(&p).unwrap();
        fs::canonicalize(&p).unwrap()
    }

    fn ts() -> String {
        "2026-09-27_211500".to_string()
    }

    #[test]
    fn beside_source_same_parent_shares_one_root() {
        let src = tempdir("sameparent");
        fs::write(src.join("a.jpg"), b"x").unwrap();
        fs::write(src.join("b.jpg"), b"y").unwrap();
        let root = job_root(&OutputMode::BesideSource, &src, &ts()).unwrap();
        assert_eq!(root, src.join(OUTPUT_DIR_NAME).join(ts()));
        let canonical = ensure_root(&root).unwrap();
        assert!(canonical.starts_with(&src));

        let pa = plan_final_path(&canonical, None, "a.cleaned.jpg").unwrap();
        let pb = plan_final_path(&canonical, None, "b.cleaned.jpg").unwrap();
        assert_eq!(pa.parent().unwrap(), pb.parent().unwrap());
        assert!(canonical.starts_with(fs::canonicalize(&src).unwrap()));
        fs::remove_dir_all(&src).unwrap();
    }

    #[test]
    fn multi_parent_batch_gets_roots_under_each_parent() {
        let base = tempdir("multiparent");
        let p1 = base.join("one");
        let p2 = base.join("two");
        fs::create_dir_all(&p1).unwrap();
        fs::create_dir_all(&p2).unwrap();
        let r1 = job_root(&OutputMode::BesideSource, &p1, &ts()).unwrap();
        let r2 = job_root(&OutputMode::BesideSource, &p2, &ts()).unwrap();
        assert!(r1.starts_with(fs::canonicalize(&p1).unwrap()));
        assert!(r2.starts_with(fs::canonicalize(&p2).unwrap()));
        assert_ne!(r1, r2);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn custom_root_layout() {
        let custom = tempdir("customroot");
        let root = job_root(&OutputMode::Custom(custom.clone()), Path::new("/irrelevant"), &ts()).unwrap();
        assert_eq!(root, custom.join(ts()));
        let canonical = ensure_root(&root).unwrap();
        let f = plan_final_path(&canonical, None, "x.cleaned.png").unwrap();
        assert_eq!(f, canonical.join("x.cleaned.png"));
        fs::remove_dir_all(&custom).unwrap();
    }

    #[test]
    fn nested_folder_relative_paths_preserved_under_root() {
        let custom = tempdir("nested");
        let root = job_root(&OutputMode::Custom(custom.clone()), Path::new("/x"), &ts()).unwrap();
        let canonical = ensure_root(&root).unwrap();
        let f = plan_final_path(&canonical, Some(Path::new("sub/deep")), "c.cleaned.pdf").unwrap();
        assert_eq!(f, canonical.join("sub/deep/c.cleaned.pdf"));
        assert!(f.starts_with(&canonical));
        fs::remove_dir_all(&custom).unwrap();
    }

    #[test]
    fn parent_dir_components_are_rejected() {
        let custom = tempdir("dotdot");
        let root = job_root(&OutputMode::Custom(custom.clone()), Path::new("/x"), &ts()).unwrap();
        let canonical = ensure_root(&root).unwrap();

        assert!(plan_final_path(&canonical, Some(Path::new("../escape")), "x.cleaned.jpg").is_err());
        assert!(plan_final_path(&canonical, Some(Path::new("sub/../../escape")), "x.cleaned.jpg").is_err());
        assert!(plan_final_path(&canonical, None, "../x.cleaned.jpg").is_err());
        assert!(plan_final_path(&canonical, None, "sub/dir/x.cleaned.jpg").is_err());
        assert!(plan_final_path(&canonical, Some(Path::new("/abs/path")), "x.cleaned.jpg").is_err());
        assert!(job_root(&OutputMode::Custom(custom.clone()), Path::new("/x"), "../evil").is_err());
        fs::remove_dir_all(&custom).unwrap();
    }

    #[test]
    fn symlinked_output_components_are_rejected() {
        let base = tempdir("symout");
        let outside = base.join("outside");
        fs::create_dir_all(&outside).unwrap();
        let root_dir = base.join("root");
        fs::create_dir_all(&root_dir).unwrap();

        // planted symlink where a subdirectory should be created
        std::os::unix::fs::symlink(&outside, root_dir.join("sub")).unwrap();
        let canonical = ensure_root(&root_dir).unwrap();
        let err = plan_final_path(&canonical, Some(Path::new("sub")), "x.cleaned.jpg");
        assert!(err.is_err(), "symlinked subdir must be rejected: {:?}", err);

        // planted symlink as the root itself
        let link_root = base.join("link-root");
        std::os::unix::fs::symlink(&outside, &link_root).unwrap();
        assert!(ensure_root(&link_root).is_err());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn collisions_never_overwrite() {
        let custom = tempdir("collision");
        let root = job_root(&OutputMode::Custom(custom.clone()), Path::new("/x"), &ts()).unwrap();
        let canonical = ensure_root(&root).unwrap();

        let first = plan_final_path(&canonical, None, "photo.cleaned.jpg").unwrap();
        fs::write(&first, b"1").unwrap();
        let second = plan_final_path(&canonical, None, "photo.cleaned.jpg").unwrap();
        assert_eq!(second.file_name().unwrap(), "photo.cleaned-2.jpg");
        fs::write(&second, b"2").unwrap();
        let third = plan_final_path(&canonical, None, "photo.cleaned.jpg").unwrap();
        assert_eq!(third.file_name().unwrap(), "photo.cleaned-3.jpg");

        // a pre-existing symlink at the target also counts as occupied
        std::os::unix::fs::symlink(&first, canonical.join("link.cleaned.jpg")).unwrap();
        let after_link = plan_final_path(&canonical, None, "link.cleaned.jpg").unwrap();
        assert_eq!(after_link.file_name().unwrap(), "link.cleaned-2.jpg");
        fs::remove_dir_all(&custom).unwrap();
    }

    #[test]
    fn read_only_target_fails_visibly() {
        let base = tempdir("readonly");
        let locked = base.join("locked");
        fs::create_dir_all(&locked).unwrap();
        let mut perms = fs::metadata(&locked).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o500);
        fs::set_permissions(&locked, perms).unwrap();

        let root = locked.join(ts());
        let err = ensure_root(&root);
        assert!(err.is_err(), "read-only base must fail: {:?}", err);

        let mut perms = fs::metadata(&locked).unwrap().permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&locked, perms).unwrap();
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn unicode_paths_roundtrip() {
        let base = tempdir("unicode");
        let src = base.join("féntè-🎉-dir");
        fs::create_dir_all(&src).unwrap();
        let root = job_root(&OutputMode::BesideSource, &src, &ts()).unwrap();
        let canonical = ensure_root(&root).unwrap();
        let f = plan_final_path(&canonical, Some(Path::new("子目录")), "报告.cleaned-🎉.docx").unwrap();
        assert!(f.exists() || f.parent().unwrap().exists());
        assert_eq!(f.file_name().unwrap(), "报告.cleaned-🎉.docx");
        assert!(f.starts_with(&canonical));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn containment_holds_for_every_planned_path() {
        let base = tempdir("containment");
        let root = job_root(&OutputMode::BesideSource, &base, &ts()).unwrap();
        let canonical = ensure_root(&root).unwrap();
        for rel in [None, Some(Path::new("a")), Some(Path::new("a/b/c"))] {
            let f = plan_final_path(&canonical, rel, "n.cleaned.ext").unwrap();
            let parent_canonical = fs::canonicalize(f.parent().unwrap()).unwrap();
            assert!(parent_canonical.starts_with(&canonical), "escape: {:?}", f);
        }
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn timestamp_format() {
        let t = timestamp_now();
        let bytes = t.as_bytes();
        assert_eq!(bytes.len(), 17, "unexpected timestamp: {t}");
        assert_eq!(bytes[4], b'-');
        assert_eq!(bytes[7], b'-');
        assert_eq!(bytes[10], b'_');
        assert!(bytes
            .iter()
            .enumerate()
            .all(|(i, b)| [4, 7].contains(&i) && *b == b'-'
                || i == 10 && *b == b'_'
                || ![4, 7, 10].contains(&i) && b.is_ascii_digit()));
    }
}
