use git2::{AttrCheckFlags, AttributeReadSession, AttributeSource, OwnedAttrValue, Repository};
use proptest::prelude::*;
use std::{fs, path::Path};

fn bundled() -> bool {
    if git2::Version::get().vendored() {
        return true;
    }
    let (_dir, mut repo) = fixture();
    assert!(AttributeReadSession::new(
        &mut repo,
        AttributeSource::Absent,
        AttrCheckFlags::default()
    )
    .err()
    .unwrap()
    .message()
    .contains("bundled libgit2"));
    false
}

fn fixture() -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path().join("repo")).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("core.attributesFile", "").unwrap();
    config.set_str("core.excludesFile", "").unwrap();
    (dir, repo)
}

#[test]
fn system_source_preloads_macros_and_retains_values() {
    if !bundled() {
        return;
    }
    let (dir, mut repo) = fixture();
    let source = dir.path().join("system");
    fs::write(
        &source,
        b"[attr]guard filter=captured\nplain filter=system\n",
    )
    .unwrap();
    fs::write(
        repo.workdir().unwrap().join(".gitattributes"),
        b"file guard\n",
    )
    .unwrap();
    let mut session = AttributeReadSession::new(
        &mut repo,
        AttributeSource::File(&source),
        AttrCheckFlags::default(),
    )
    .unwrap();
    fs::remove_file(&source).unwrap();
    let owned = session.get(Path::new("file"), false, "filter").unwrap();
    assert_eq!(owned, OwnedAttrValue::Bytes(b"captured".to_vec()));
    assert_eq!(
        session.get(Path::new("plain"), false, "filter").unwrap(),
        OwnedAttrValue::Bytes(b"system".to_vec())
    );
    drop(session);
    assert_eq!(owned, OwnedAttrValue::Bytes(b"captured".to_vec()));
    assert!(AttributeReadSession::new(
        &mut repo,
        AttributeSource::File(&source),
        AttrCheckFlags::default(),
    )
    .is_err());
    let mut absent = AttributeReadSession::new(
        &mut repo,
        AttributeSource::Absent,
        AttrCheckFlags::default(),
    )
    .unwrap();
    assert_eq!(
        absent.get(Path::new("file"), false, "filter").unwrap(),
        OwnedAttrValue::Unspecified
    );
}

#[test]
fn captured_directory_kind_does_not_require_an_operand() {
    if !bundled() {
        return;
    }
    let (_dir, mut repo) = fixture();
    fs::write(
        repo.workdir().unwrap().join(".gitattributes"),
        b"folder/ filter=directory\n",
    )
    .unwrap();
    let mut session = AttributeReadSession::new(
        &mut repo,
        AttributeSource::Absent,
        AttrCheckFlags::default(),
    )
    .unwrap();
    assert_eq!(
        session.get(Path::new("folder"), false, "filter").unwrap(),
        OwnedAttrValue::Unspecified
    );
    assert_eq!(
        session.get(Path::new("folder"), true, "filter").unwrap(),
        OwnedAttrValue::Bytes(b"directory".to_vec())
    );
}

#[test]
fn system_scope_is_independent_of_its_worktree_location() {
    if !bundled() {
        return;
    }
    let (_dir, mut repo) = fixture();
    let workdir = repo.workdir().unwrap();
    fs::create_dir(workdir.join("policy")).unwrap();
    let source = workdir.join("policy/.gitattributes");
    fs::write(&source, b"/file.txt filter=system\n").unwrap();
    let mut session = AttributeReadSession::new(
        &mut repo,
        AttributeSource::File(&source),
        AttrCheckFlags::default(),
    )
    .unwrap();
    assert_eq!(
        session.get(Path::new("file.txt"), false, "filter").unwrap(),
        OwnedAttrValue::Bytes(b"system".to_vec())
    );
    assert_eq!(
        session
            .get(Path::new("policy/file.txt"), false, "filter")
            .unwrap(),
        OwnedAttrValue::Bytes(b"system".to_vec())
    );
    assert_eq!(
        session.get(Path::new("file.txt"), false, "filter").unwrap(),
        OwnedAttrValue::Bytes(b"system".to_vec())
    );
}

#[test]
fn existing_cache_is_rejected_without_invalidating_its_values() {
    if !bundled() {
        return;
    }
    let (_dir, mut repo) = fixture();
    fs::write(
        repo.workdir().unwrap().join(".gitattributes"),
        b"file filter=before\n",
    )
    .unwrap();
    assert!(repo
        .get_attr_bytes(Path::new("file"), "filter", AttrCheckFlags::default())
        .unwrap()
        .is_some());
    assert!(AttributeReadSession::new(
        &mut repo,
        AttributeSource::Absent,
        AttrCheckFlags::default()
    )
    .is_err());
    assert_eq!(
        repo.get_attr_bytes(Path::new("file"), "filter", AttrCheckFlags::default())
            .unwrap(),
        Some(b"before".as_slice())
    );
}

#[test]
fn simultaneous_sessions_do_not_share_macros() {
    if !bundled() {
        return;
    }
    let (first, mut left) = fixture();
    let (second, mut right) = fixture();
    let left_source = first.path().join("system");
    let right_source = second.path().join("system");
    for (repo, path, bytes) in [
        (&left, &left_source, "[attr]guard filter=left\n"),
        (&right, &right_source, "[attr]guard filter=right\n"),
    ] {
        fs::write(path, bytes).unwrap();
        fs::write(
            repo.workdir().unwrap().join(".gitattributes"),
            b"file guard\n",
        )
        .unwrap();
    }
    let mut a = AttributeReadSession::new(
        &mut left,
        AttributeSource::File(&left_source),
        AttrCheckFlags::default(),
    )
    .unwrap();
    let mut b = AttributeReadSession::new(
        &mut right,
        AttributeSource::File(&right_source),
        AttrCheckFlags::default(),
    )
    .unwrap();
    assert_eq!(
        a.get(Path::new("file"), false, "filter").unwrap(),
        OwnedAttrValue::Bytes(b"left".to_vec())
    );
    assert_eq!(
        b.get(Path::new("file"), false, "filter").unwrap(),
        OwnedAttrValue::Bytes(b"right".to_vec())
    );
    drop(a);
    assert_eq!(
        b.get(Path::new("file"), false, "filter").unwrap(),
        OwnedAttrValue::Bytes(b"right".to_vec())
    );
}

#[test]
fn outstanding_index_handle_prevents_session_acquisition() {
    if !bundled() {
        return;
    }
    let (_dir, mut repo) = fixture();
    let index = repo.index().unwrap();
    let second = repo.index().unwrap();
    let error = AttributeReadSession::new(
        &mut repo,
        AttributeSource::Absent,
        AttrCheckFlags::default(),
    )
    .err()
    .expect("an outstanding index must prevent exclusive cache ownership");
    assert!(error.message().contains("index"));
    drop(index);
    assert!(AttributeReadSession::new(
        &mut repo,
        AttributeSource::Absent,
        AttrCheckFlags::default(),
    )
    .is_err());
    drop(second);
    let mut session = AttributeReadSession::new(
        &mut repo,
        AttributeSource::Absent,
        AttrCheckFlags::default(),
    )
    .unwrap();
    assert_eq!(
        session.get(Path::new("file"), false, "filter").unwrap(),
        OwnedAttrValue::Unspecified
    );
}

#[test]
fn invalid_inputs_and_unwind_release_the_owned_cache() {
    if !bundled() {
        return;
    }
    let (dir, mut repo) = fixture();
    assert!(AttributeReadSession::new(
        &mut repo,
        AttributeSource::File(Path::new("relative")),
        AttrCheckFlags::default()
    )
    .is_err());
    let nul = dir.path().join("nul\0file");
    assert!(AttributeReadSession::new(
        &mut repo,
        AttributeSource::File(&nul),
        AttrCheckFlags::default()
    )
    .is_err());
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut session = AttributeReadSession::new(
            &mut repo,
            AttributeSource::Absent,
            AttrCheckFlags::default(),
        )
        .unwrap();
        assert!(session
            .get(Path::new("nul\0file"), false, "filter")
            .is_err());
        assert!(session.get(Path::new("file"), false, "nul\0name").is_err());
        assert!(session.get(dir.path(), false, "filter").is_err());
        panic!("exercise session cleanup during unwind");
    }));
    assert!(failure.is_err());
    let mut session = AttributeReadSession::new(
        &mut repo,
        AttributeSource::Absent,
        AttrCheckFlags::default(),
    )
    .unwrap();
    assert_eq!(
        session.get(Path::new("file"), false, "filter").unwrap(),
        OwnedAttrValue::Unspecified
    );
}

#[test]
fn file_and_index_sources_retain_native_order_and_root_macros() {
    if !bundled() {
        return;
    }
    let (_dir, mut repo) = fixture();
    let root = repo.workdir().unwrap().to_owned();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(
        root.join(".gitattributes"),
        b"[attr]guard filter=index\nfile filter=index\n",
    )
    .unwrap();
    {
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(".gitattributes")).unwrap();
        index.write().unwrap();
    }
    fs::write(
        root.join(".gitattributes"),
        b"[attr]guard filter=worktree\nfile filter=worktree\n",
    )
    .unwrap();
    fs::write(root.join("sub/.gitattributes"), b"file guard\n").unwrap();
    // Index insertion used the ordinary attribute parser. A read session
    // intentionally starts with a fresh handle, not that populated cache.
    repo = Repository::open(&root).unwrap();
    for (flags, expected) in [
        (AttrCheckFlags::FILE_THEN_INDEX, b"worktree".as_slice()),
        (AttrCheckFlags::INDEX_THEN_FILE, b"index".as_slice()),
        (AttrCheckFlags::INDEX_ONLY, b"index".as_slice()),
    ] {
        let mut session =
            AttributeReadSession::new(&mut repo, AttributeSource::Absent, flags).unwrap();
        assert_eq!(
            session.get(Path::new("file"), false, "filter").unwrap(),
            OwnedAttrValue::Bytes(expected.to_vec())
        );
    }
    let mut session = AttributeReadSession::new(
        &mut repo,
        AttributeSource::Absent,
        AttrCheckFlags::default(),
    )
    .unwrap();
    assert_eq!(
        session.get(Path::new("sub/file"), false, "filter").unwrap(),
        OwnedAttrValue::Bytes(b"index".to_vec())
    );
}

#[cfg(windows)]
#[test]
fn windows_absolute_sources_preserve_native_path_forms() {
    if !bundled() {
        return;
    }
    let (dir, mut repo) = fixture();
    let source = dir.path().join("system");
    fs::write(&source, b"* filter=selected\n").unwrap();
    let extended = fs::canonicalize(&source).unwrap();
    let mut session = AttributeReadSession::new(
        &mut repo,
        AttributeSource::File(&extended),
        AttrCheckFlags::default(),
    )
    .unwrap();
    fs::remove_file(source).unwrap();
    assert_eq!(
        session.get(Path::new("file"), false, "filter").unwrap(),
        OwnedAttrValue::Bytes(b"selected".to_vec())
    );
    for rooted in [r"\\server\share\file", r"\file", r"C:\file"] {
        assert!(session.get(Path::new(rooted), false, "filter").is_err());
    }
    drop(session);
    repo.get_attr_bytes(Path::new("file"), "filter", AttrCheckFlags::default())
        .unwrap();
    for unc in [r"\\server\share\attrs", "//server/share/attrs"] {
        let error = AttributeReadSession::new(
            &mut repo,
            AttributeSource::File(Path::new(unc)),
            AttrCheckFlags::default(),
        )
        .err()
        .expect("the populated cache must refuse before opening a network source");
        assert!(error.message().contains("fresh cache"));
    }
}

#[cfg(unix)]
#[test]
fn regular_symlinks_and_undecodable_paths_work_but_devices_refuse() {
    use std::os::unix::{ffi::OsStrExt, fs::symlink};
    if !bundled() {
        return;
    }
    let (dir, mut repo) = fixture();
    let target = dir.path().join("source");
    let alias = dir.path().join("alias");
    symlink(&target, &alias).unwrap();
    for bytes in [b"".as_slice(), b"* filter=\xff\n"] {
        fs::write(&target, bytes).unwrap();
        let mut session = AttributeReadSession::new(
            &mut repo,
            AttributeSource::File(&alias),
            AttrCheckFlags::default(),
        )
        .unwrap();
        let value = session
            .get(
                Path::new(std::ffi::OsStr::from_bytes(b"operand-\xfe")),
                false,
                "filter",
            )
            .unwrap();
        assert_eq!(
            value,
            if bytes.is_empty() {
                OwnedAttrValue::Unspecified
            } else {
                OwnedAttrValue::Bytes(vec![0xff])
            }
        );
    }
    let device = dir.path().join("device");
    symlink("/dev/null", &device).unwrap();
    assert!(AttributeReadSession::new(
        &mut repo,
        AttributeSource::File(&device),
        AttrCheckFlags::default()
    )
    .is_err());
}

fn attribute_assignment() -> impl Strategy<Value = (Vec<u8>, OwnedAttrValue)> {
    let bytes = prop_oneof![
        3 => Just(b"[internal]__TRUE__".to_vec()),
        6 => proptest::collection::vec(prop_oneof![2 => Just(0xffu8), 1 => Just(b'='), 6 => b'a'..=b'z'], 1..65),
    ];
    prop_oneof![
        2 => Just((b"filter".to_vec(), OwnedAttrValue::True)),
        2 => Just((b"-filter".to_vec(), OwnedAttrValue::False)),
        2 => Just((b"!filter".to_vec(), OwnedAttrValue::Unspecified)),
        3 => Just((b"filter=".to_vec(), OwnedAttrValue::True)),
        6 => bytes.prop_map(|value| {
            let mut assignment = b"filter=".to_vec();
            assignment.extend_from_slice(&value);
            (assignment, OwnedAttrValue::Bytes(value))
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Direct("tests/attribute_session.proptest-regressions"))),
        .. ProptestConfig::default()
    })]
    #[test]
    fn owned_values_preserve_native_states_and_bytes((assignment, expected) in attribute_assignment()) {
        if !bundled() { return Ok(()); }
        let (_dir, mut repo) = fixture();
        let mut rule = b"file ".to_vec();
        rule.extend(assignment);
        rule.push(b'\n');
        fs::write(repo.workdir().unwrap().join(".gitattributes"), rule).unwrap();
        let mut session = AttributeReadSession::new(&mut repo, AttributeSource::Absent, AttrCheckFlags::default()).unwrap();
        let value = session.get(Path::new("file"), false, "filter").unwrap();
        drop(session);
        prop_assert_eq!(value, expected);
    }
}
