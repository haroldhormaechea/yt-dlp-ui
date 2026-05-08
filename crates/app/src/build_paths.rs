//! Path-resolution helpers shared between `build.rs` and runtime code.
//!
//! The canonical home for these helpers is here; `build.rs` keeps a verbatim
//! copy because Cargo build scripts cannot depend on the crate they build.
//! Both copies must stay in lock-step.
//
// MIRROR: crates/app/build.rs

// CONVENTION: Cargo places build-script outputs at
//   target/<...>/<profile>/build/<crate>-<hash>/out
// We climb four ancestors of OUT_DIR (out -> <crate>-<hash> -> build -> <profile>)
// to reach the profile dir. This is Cargo convention, not a stable contract;
// a future Cargo layout change would silently misroute bundling.
// The unit tests in crates/app/src/build_paths_test.rs lock the parser to
// the documented layout.

use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "build_paths_test.rs"]
mod build_paths_tests;

/// Resolves the cargo profile output directory from a build-script `OUT_DIR`.
///
/// Returns `None` if any of the four ancestor climbs fails (e.g. the input is
/// shorter than expected by the documented layout).
//
// This module exists to host the unit tests for the climb-back logic; the
// runtime callsite lives in `build.rs` (which keeps a verbatim copy because
// build scripts cannot depend on the crate they build).
#[allow(dead_code)]
pub(crate) fn profile_dir_from_out_dir(out_dir: &Path) -> Option<PathBuf> {
    let crate_hash_dir = out_dir.parent()?;
    let build_dir = crate_hash_dir.parent()?;
    let profile_dir = build_dir.parent()?;
    // Confirm the layout: the parent of `out` should be `<crate>-<hash>`,
    // and the directory two levels up should be named `build`. We don't
    // assert on the profile name (`debug` / `release` / custom) because
    // custom profiles are valid.
    if build_dir.file_name().and_then(|n| n.to_str()) != Some("build") {
        return None;
    }
    Some(profile_dir.to_path_buf())
}
