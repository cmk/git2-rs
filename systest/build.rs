use std::env;
use std::path::PathBuf;

fn main() {
    let mut cfg = ctest2::TestGenerator::new();
    cfg.include(".").header("abi.h");
    println!("cargo:rerun-if-changed=abi.h");
    let vendored =
        env::var("DEP_GIT2_VENDORED").expect("libgit2-sys must report its selected linkage") == "1";
    if let Some(root) = env::var_os("DEP_GIT2_ROOT") {
        let root = PathBuf::from(root);
        cfg.include(if vendored { root.join("include") } else { root });
    }
    // Stock system headers do not declare this fork's optional flag. The safe
    // Rust setter rejects that linkage; bundled builds must check the constant.
    cfg.skip_const(move |name| !vendored && name == "GIT_MERGE_SKIP_ATTRIBUTES");
    if vendored {
        cfg.cfg("libgit2_vendored", None);
    }
    cfg.skip_fn(move |name| {
        !vendored
            && (name.starts_with("git_ignore_source_")
                || name.starts_with("git_attr_read_session_")
                || name == "git_repository_precompose_workdir_path"
                || name == "git_repository_open_gitdir_preserving_path")
    });

    // Enable the unstable-sha256 rust cfg
    // so ctest2 sees the correct function signatures
    if env::var("CARGO_FEATURE_UNSTABLE_SHA256").is_ok() {
        cfg.cfg("feature", Some("unstable-sha256"));
        cfg.define("GIT_EXPERIMENTAL_SHA256", Some("1"));
    }

    cfg.header("git2.h")
        .header("git2/sys/errors.h")
        .header("git2/sys/transport.h")
        .header("git2/sys/refs.h")
        .header("git2/sys/refdb_backend.h")
        .header("git2/sys/odb_backend.h")
        .header("git2/sys/mempack.h")
        .header("git2/sys/repository.h")
        .header("git2/sys/cred.h")
        .header("git2/sys/email.h")
        .header("git2/sys/config.h")
        .header("git2/cred_helpers.h")
        .type_name(|s, _, _| s.to_string());
    cfg.field_name(|_, f| match f {
        "kind" => "type".to_string(),
        _ => f.to_string(),
    });
    cfg.skip_field(|struct_, f| {
        // this field is marked as const which ctest complains about
        (struct_ == "git_rebase_operation" && f == "id") ||
        // the real name of this field is ref but that is a reserved keyword
        (struct_ == "git_worktree_add_options" && f == "reference")
    });
    cfg.skip_signededness(|s| match s {
        s if s.ends_with("_cb") => true,
        s if s.ends_with("_callback") => true,
        "git_push_transfer_progress" | "git_push_negotiation" | "git_packbuilder_progress" => true,
        _ => false,
    });

    // GIT_FILEMODE_BLOB_GROUP_WRITABLE is not a public const in libgit2
    cfg.define("GIT_FILEMODE_BLOB_GROUP_WRITABLE", Some("0100664"));

    // not entirely sure why this is failing...
    cfg.skip_roundtrip(|t| t == "git_clone_options" || t == "git_submodule_update_options");

    cfg.skip_type(move |t| {
        t == "__enum_ty"
            || (!vendored && matches!(t, "git_ignore_source" | "git_attr_read_session"))
    });
    cfg.generate("../libgit2-sys/lib.rs", "all.rs");
}
