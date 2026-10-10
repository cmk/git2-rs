use std::marker::PhantomData;
use std::path::Path;
use std::ptr;

use crate::util::{self, Binding};
use crate::{raw, Error, Repository};

/// An operation-local replacement for the repository's global ignore file.
///
/// Captured bytes are parsed once with their original pathname and the
/// repository's current case policy. Local ignore files, info/exclude and
/// internal rules retain their ordinary precedence and live lookup behavior.
/// This value neither changes the repository nor opens the original source.
/// Keep the repository's workdir and ignore settings unchanged while using it.
pub struct IgnoreSource<'repo> {
    raw: *mut raw::git_ignore_source,
    _repo: PhantomData<&'repo Repository>,
}

impl<'repo> IgnoreSource<'repo> {
    /// Parse captured global rules using their original source pathname.
    ///
    /// Relative source paths are workdir-relative. An empty buffer disables
    /// global rules, including ambient defaults. Embedded NUL and UTF-8 BOM
    /// handling is the native file parser's. Bytes and pathname are copied.
    /// Stock system libgit2 linkage returns an error before allocating.
    pub fn new(repo: &'repo Repository, origin: &Path, bytes: &[u8]) -> Result<Self, Error> {
        if !raw::vendored() {
            return Err(Error::from_str(
                "captured ignore sources require bundled libgit2",
            ));
        }
        let origin = util::cstring_to_repo_path(origin)?;
        let mut out = ptr::null_mut();
        unsafe {
            try_call!(raw::git_ignore_source_new(
                &mut out,
                repo.raw(),
                origin,
                bytes.as_ptr().cast(),
                bytes.len()
            ));
        }
        Ok(Self {
            raw: out,
            _repo: PhantomData,
        })
    }

    /// Check a workdir-relative path with this captured global source.
    pub fn is_path_ignored(&self, path: &Path) -> Result<bool, Error> {
        let path = util::cstring_to_repo_path(path)?;
        let mut ignored = 0;
        unsafe {
            try_call!(raw::git_ignore_source_path_is_ignored(
                &mut ignored,
                self.raw,
                path
            ));
        }
        Ok(ignored == 1)
    }
}

impl Drop for IgnoreSource<'_> {
    fn drop(&mut self) {
        unsafe { raw::git_ignore_source_free(self.raw) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_global_source_matches_native_context_and_parser() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let repo = Repository::init(&root).unwrap();
        if !raw::vendored() {
            assert!(IgnoreSource::new(&repo, Path::new("rules"), b"").is_err());
            return;
        }
        std::fs::create_dir(root.join("sub")).unwrap();
        let workdir = repo.workdir().unwrap();
        let alias = workdir
            .parent()
            .unwrap()
            .join(".")
            .join(workdir.file_name().unwrap());
        for origin in ["sub/.gitignore", "sub/rules", "rules"] {
            let path = root.join(origin);
            let sources = [
                Path::new(origin).to_path_buf(),
                workdir.join(origin),
                path.clone(),
                alias.join(origin),
            ];
            for bytes in [
                b"/root-only\n".as_slice(),
                b"\xef\xbb\xbf/root-only\r\n".as_slice(),
                b"*.tmp\n!keep.tmp\n".as_slice(),
                b"!root-only\n".as_slice(),
                b"root-only\0*.tmp\n".as_slice(),
                b"".as_slice(),
            ] {
                for case in [false, true] {
                    std::fs::write(&path, bytes).unwrap();
                    repo.config()
                        .unwrap()
                        .set_bool("core.ignoreCase", case)
                        .unwrap();
                    for selected in &sources {
                        // Ignore scope depends on the source spelling, not just
                        // file identity. Give the oracle the same slash-normalized
                        // pathname as the safe wrapper, including Windows prefixes.
                        let configured = util::cstring_to_repo_path(selected.as_path()).unwrap();
                        repo.config()
                            .unwrap()
                            .set_str("core.excludesFile", configured.to_str().unwrap())
                            .unwrap();
                        let native = Repository::open(&root).unwrap();
                        let captured = IgnoreSource::new(&native, selected, bytes).unwrap();
                        for name in [
                            "root-only",
                            "ROOT-ONLY",
                            "sub/root-only",
                            "x.tmp",
                            "keep.tmp",
                            "sub/x.tmp",
                        ] {
                            assert_eq!(
                                captured.is_path_ignored(Path::new(name)).unwrap(),
                                native.is_path_ignored(name).unwrap(),
                                "{selected:?}: {name}, {bytes:?}, case={case}"
                            );
                        }
                    }
                }
            }
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn captured_global_source_is_owned_isolated_and_low_priority() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let repo = Repository::init(&root).unwrap();
        if !raw::vendored() {
            assert!(IgnoreSource::new(&repo, Path::new("rules"), b"").is_err());
            return;
        }
        let origin = root.join("rules");
        std::fs::write(&origin, b"/live\n").unwrap();
        repo.config()
            .unwrap()
            .set_str("core.excludesFile", origin.to_str().unwrap())
            .unwrap();
        std::fs::write(root.join(".gitignore"), b"*.tmp\n!keep.tmp\n").unwrap();
        std::fs::write(repo.path().join("info/exclude"), b"*.info\n!keep.info\n").unwrap();
        let bytes = b"/captured\n*.tmp\n*.info\nparent/\n".to_vec();
        let captured = IgnoreSource::new(&repo, &origin, &bytes).unwrap();
        let empty = IgnoreSource::new(&repo, &origin, b"").unwrap();
        drop(bytes);
        for _ in 0..3 {
            assert!(repo.is_path_ignored("live").unwrap());
            assert!(!captured.is_path_ignored(Path::new("live")).unwrap());
            assert!(captured.is_path_ignored(Path::new("captured")).unwrap());
            assert!(captured.is_path_ignored(Path::new("parent/child")).unwrap());
            assert!(!captured.is_path_ignored(Path::new("keep.tmp")).unwrap());
            assert!(!captured.is_path_ignored(Path::new("keep.info")).unwrap());
            assert!(!empty.is_path_ignored(Path::new("live")).unwrap());
            assert!(!empty.is_path_ignored(Path::new("captured")).unwrap());
            assert!(empty.is_path_ignored(Path::new("x.tmp")).unwrap());
        }
        drop(empty);
        std::fs::remove_file(&origin).unwrap();
        assert!(captured.is_path_ignored(Path::new("captured")).unwrap());
        repo.add_ignore_rule("/internal\n").unwrap();
        assert!(captured.is_path_ignored(Path::new("internal")).unwrap());
        drop(captured);
        assert!(!repo.is_path_ignored("captured").unwrap());
    }
}
