//! Unit tests for [`crate::build_paths::profile_dir_from_out_dir`] and a
//! duplicated copy of the build-script's idempotent install helper.
//!
//! `build.rs` and `crate::build_paths` keep mirrored copies of the path
//! resolver because Cargo build scripts cannot depend on the crate they
//! build. The helper used during build also cannot be reached from
//! `crates/*/src/**/*_test.rs`, so the install-helper logic is duplicated
//! here verbatim and exercised against a tempdir. The proposal authorized
//! this duplication.

use std::path::{Path, PathBuf};

use tempfile::tempdir;

use crate::build_paths::profile_dir_from_out_dir;

// ---------------------------------------------------------------------------
// profile_dir_from_out_dir — layout climb-back
// ---------------------------------------------------------------------------

#[test]
fn debug_layout_resolves_to_target_debug() {
    let out_dir = PathBuf::from("/work/proj/target/debug/build/app-deadbeef/out");
    let profile = profile_dir_from_out_dir(&out_dir).expect("layout should resolve");
    assert_eq!(profile, PathBuf::from("/work/proj/target/debug"));
}

#[test]
fn release_layout_resolves_to_target_release() {
    let out_dir = PathBuf::from("/work/proj/target/release/build/app-cafef00d/out");
    let profile = profile_dir_from_out_dir(&out_dir).expect("layout should resolve");
    assert_eq!(profile, PathBuf::from("/work/proj/target/release"));
}

#[test]
fn target_triple_layout_resolves_to_triple_profile_dir() {
    let out_dir =
        PathBuf::from("/work/proj/target/aarch64-apple-darwin/debug/build/app-1234abcd/out");
    let profile = profile_dir_from_out_dir(&out_dir).expect("layout should resolve");
    assert_eq!(
        profile,
        PathBuf::from("/work/proj/target/aarch64-apple-darwin/debug")
    );
}

#[test]
fn custom_profile_layout_is_accepted() {
    // Custom profiles are valid (e.g. `[profile.dist]`); the resolver should
    // not assert on the profile name, only on the literal `build` directory.
    let out_dir = PathBuf::from("/work/proj/target/dist/build/app-aaaaaaaa/out");
    let profile = profile_dir_from_out_dir(&out_dir).expect("custom profile should resolve");
    assert_eq!(profile, PathBuf::from("/work/proj/target/dist"));
}

#[test]
fn input_too_shallow_returns_none() {
    // Fewer than three parent levels available — the climb-back can't satisfy
    // the documented `out → <crate>-<hash> → build → <profile>` layout.
    assert!(profile_dir_from_out_dir(&PathBuf::from("/")).is_none());
    assert!(profile_dir_from_out_dir(&PathBuf::from("/out")).is_none());
    // Two ancestors only (parent of `out` exists, but its parent is `/` which
    // has no parent — `build_dir.parent()?` returns None).
    assert!(profile_dir_from_out_dir(&PathBuf::from("/x/out")).is_none());
}

#[test]
fn parent_of_crate_hash_must_be_named_build() {
    // The grandparent of `out` must literally be named `build`. If a future
    // Cargo layout change names it something else, we want to bail early
    // rather than silently misroute bundling.
    let out_dir = PathBuf::from("/work/proj/target/debug/scripts/app-deadbeef/out");
    assert!(
        profile_dir_from_out_dir(&out_dir).is_none(),
        "non-`build` parent should not resolve"
    );
}

// ---------------------------------------------------------------------------
// install_wrapper — idempotency
//
// This is a verbatim duplicate of the helper in `crates/app/build.rs`. Build
// scripts cannot be reached from `*_test.rs`, so the proposal authorized
// duplicating the small helper into the test file and exercising it here.
// Keep this in lock-step with `build.rs::install_wrapper`.
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn install_wrapper(source: &Path, dest: &Path) -> Result<(), std::io::Error> {
    use std::fs;

    if dest.is_symlink() {
        let source_canon = source.canonicalize().ok();
        let dest_target_canon = dest.read_link().ok().and_then(|t| {
            let resolved = if t.is_absolute() {
                t
            } else {
                dest.parent().map_or(t.clone(), |p| p.join(t))
            };
            resolved.canonicalize().ok()
        });
        if source_canon.is_some() && source_canon == dest_target_canon {
            return Ok(());
        }
    }

    if dest.exists() || dest.is_symlink() {
        fs::remove_file(dest)?;
    }
    std::os::unix::fs::symlink(source, dest)
}

#[cfg(windows)]
fn install_wrapper(source: &Path, dest: &Path) -> Result<(), std::io::Error> {
    use std::fs;

    if dest.is_file() {
        let source_meta = fs::metadata(source).ok();
        let dest_meta = fs::metadata(dest).ok();
        if let (Some(s), Some(d)) = (source_meta.as_ref(), dest_meta.as_ref()) {
            let same_len = s.len() == d.len();
            let same_mtime = matches!(
                (s.modified(), d.modified()),
                (Ok(sm), Ok(dm)) if sm == dm
            );
            if same_len && same_mtime {
                return Ok(());
            }
        }
    }

    fs::copy(source, dest).map(|_| ())
}

#[cfg(unix)]
#[test]
fn install_wrapper_creates_symlink_when_dest_missing() {
    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("yt-dlp.sh");
    std::fs::write(&source, "#!/bin/sh\necho potato\n").expect("write source");
    let dest = dir.path().join("yt-dlp");

    install_wrapper(&source, &dest).expect("install");

    assert!(dest.is_symlink(), "dest must be a symlink");
    let target = std::fs::read_link(&dest).expect("read_link");
    let resolved = if target.is_absolute() {
        target
    } else {
        dest.parent().unwrap().join(target)
    };
    assert_eq!(
        resolved.canonicalize().unwrap(),
        source.canonicalize().unwrap(),
        "symlink must point at source"
    );
}

#[cfg(unix)]
#[test]
fn install_wrapper_is_idempotent_when_symlink_already_correct() {
    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("yt-dlp.sh");
    std::fs::write(&source, "#!/bin/sh\necho potato\n").expect("write source");
    let dest = dir.path().join("yt-dlp");

    install_wrapper(&source, &dest).expect("first install");
    let first_meta = std::fs::symlink_metadata(&dest).expect("meta after first");
    // ensure mtime resolution doesn't make the comparison spurious
    std::thread::sleep(std::time::Duration::from_millis(10));
    install_wrapper(&source, &dest).expect("second install");
    let second_meta = std::fs::symlink_metadata(&dest).expect("meta after second");

    assert_eq!(
        first_meta.modified().ok(),
        second_meta.modified().ok(),
        "second install must be a no-op (symlink mtime unchanged)"
    );
    assert!(dest.is_symlink(), "dest still a symlink");
}

#[cfg(unix)]
#[test]
fn install_wrapper_replaces_stale_symlink() {
    let dir = tempdir().expect("tempdir");
    let stale_source = dir.path().join("old.sh");
    std::fs::write(&stale_source, "old\n").expect("write stale");
    let new_source = dir.path().join("yt-dlp.sh");
    std::fs::write(&new_source, "new\n").expect("write new");
    let dest = dir.path().join("yt-dlp");

    install_wrapper(&stale_source, &dest).expect("install stale");
    assert!(dest.is_symlink());

    install_wrapper(&new_source, &dest).expect("install new");
    let target = std::fs::read_link(&dest).expect("read_link");
    let resolved = if target.is_absolute() {
        target
    } else {
        dest.parent().unwrap().join(target)
    };
    assert_eq!(
        resolved.canonicalize().unwrap(),
        new_source.canonicalize().unwrap(),
        "symlink must now point at the new source"
    );
}

#[cfg(windows)]
#[test]
fn install_wrapper_copies_when_dest_missing() {
    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("yt-dlp.cmd");
    std::fs::write(&source, b"@echo potato\r\n").expect("write source");
    let dest = dir.path().join("yt-dlp.cmd");
    // sanity: source and dest must not collide in the test (different parents)
    let dest_dir = dir.path().join("nested");
    std::fs::create_dir_all(&dest_dir).expect("nested dir");
    let dest = dest_dir.join("yt-dlp.cmd");

    install_wrapper(&source, &dest).expect("install");

    assert!(dest.is_file(), "dest must be a regular file");
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        std::fs::read(&source).unwrap(),
        "copy must match source bytes"
    );
}

#[cfg(windows)]
#[test]
fn install_wrapper_is_idempotent_when_copy_already_matches() {
    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("yt-dlp.cmd");
    std::fs::write(&source, b"@echo potato\r\n").expect("write source");
    let dest_dir = dir.path().join("nested");
    std::fs::create_dir_all(&dest_dir).expect("nested dir");
    let dest = dest_dir.join("yt-dlp.cmd");

    install_wrapper(&source, &dest).expect("first install");
    let first_meta = std::fs::metadata(&dest).expect("meta after first");
    std::thread::sleep(std::time::Duration::from_millis(10));
    install_wrapper(&source, &dest).expect("second install");
    let second_meta = std::fs::metadata(&dest).expect("meta after second");

    assert_eq!(
        first_meta.modified().ok(),
        second_meta.modified().ok(),
        "second install must skip the copy (mtime unchanged)"
    );
}
