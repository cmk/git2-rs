#![cfg(target_os = "linux")]

use git2::{AttrCheckFlags, AttributeReadSession, AttributeSource, OwnedAttrValue, Repository};
use std::{ffi::OsStr, fs, os::unix::ffi::OsStrExt, path::Path};

#[test]
fn system_source_path_preserves_non_utf8_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join(OsStr::from_bytes(b"source-\xff"));
    fs::write(&source, b"* filter=selected\n").unwrap();
    let mut repo = Repository::init(dir.path().join("repo")).unwrap();
    repo.config()
        .unwrap()
        .set_str("core.attributesFile", "")
        .unwrap();
    repo.config()
        .unwrap()
        .set_str("core.excludesFile", "")
        .unwrap();
    let session = AttributeReadSession::new(
        &mut repo,
        AttributeSource::File(&source),
        AttrCheckFlags::default(),
    );
    if !git2::Version::get().vendored() {
        assert!(session.is_err());
        return;
    }
    assert_eq!(
        session
            .unwrap()
            .get(Path::new("file"), false, "filter")
            .unwrap(),
        OwnedAttrValue::Bytes(b"selected".to_vec())
    );
}
