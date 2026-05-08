//! Compiles the Slint UI markup at build time. The generated Rust module is
//! injected into `lib.rs` via `slint::include_modules!()`.
//!
//! Also bundles the upstream `yt-dlp` wrapper next to the cargo-produced
//! binary so `cargo run` works on a fresh clone without a separate yt-dlp
//! install. See `bundle_yt_dlp_wrapper_for_dev` below.
//
// MIRROR: crates/app/src/build_paths.rs

use std::path::{Path, PathBuf};

fn main() {
    slint_build::compile("ui/main_window.slint").expect("failed to compile Slint UI");
    bundle_yt_dlp_wrapper_for_dev();
    bundle_dev_ffmpeg();
    bake_runtime_pins();
}

/// Places the upstream `yt-dlp` wrapper script next to the compiled `app`
/// binary so the dev build can invoke yt-dlp without a separate install.
///
/// Unix: symlinks `<workspace>/yt-dlp.sh` to `<profile_dir>/yt-dlp`.
/// Windows: copies `<workspace>\yt-dlp.cmd` to `<profile_dir>\yt-dlp.cmd`.
///
/// All failures are reported as `cargo:warning=` and the function returns
/// early; we never panic the build script.
fn bundle_yt_dlp_wrapper_for_dev() {
    // These directives MUST be emitted on every build-script invocation, even
    // when the gate below fires and we early-return. Otherwise cargo never
    // learns the env vars are build inputs, and the gate gets sticky: a build
    // with `CARGO_DIST_TARGET` set short-circuits, then subsequent builds with
    // it unset do not re-run the script and the symlink stays missing.
    println!("cargo:rerun-if-env-changed=CARGO_DIST_TARGET");
    println!("cargo:rerun-if-env-changed=DIST_TARGET");
    println!("cargo:rerun-if-env-changed=PROFILE");

    // VERIFY: These env-var names were assumed during analyst/challenger
    // review on 2026-04-25 and not empirically confirmed against
    // cargo-dist v0.31.0. Before merge, run `dist build` once locally
    // and inspect the env passed to build scripts (e.g., via a temporary
    // `eprintln!("env={:?}", std::env::vars().collect::<Vec<_>>());`).
    // If the actual marker name differs from both, update the lookup AND
    // the `rerun-if-env-changed` directives above.
    if std::env::var_os("CARGO_DIST_TARGET").is_some() || std::env::var_os("DIST_TARGET").is_some()
    {
        println!("cargo:warning=skipping dev wrapper bundling under cargo-dist (release pipeline)");
        return;
    }

    let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from) else {
        println!("cargo:warning=CARGO_MANIFEST_DIR not set; skipping dev wrapper bundling");
        return;
    };

    let Some(workspace_root) = find_workspace_root(&manifest_dir) else {
        println!(
            "cargo:warning=could not locate workspace root from {}; skipping dev wrapper bundling",
            manifest_dir.display()
        );
        return;
    };

    let Some(out_dir) = std::env::var_os("OUT_DIR").map(PathBuf::from) else {
        println!("cargo:warning=OUT_DIR not set; skipping dev wrapper bundling");
        return;
    };

    let Some(profile_dir) = profile_dir_from_out_dir(&out_dir) else {
        println!(
            "cargo:warning=could not resolve cargo profile dir from OUT_DIR ({}); skipping dev wrapper bundling",
            out_dir.display()
        );
        return;
    };

    #[cfg(unix)]
    let (source, dest) = (workspace_root.join("yt-dlp.sh"), profile_dir.join("yt-dlp"));
    #[cfg(windows)]
    let (source, dest) = (
        workspace_root.join("yt-dlp.cmd"),
        profile_dir.join("yt-dlp.cmd"),
    );

    if !source.is_file() {
        println!(
            "cargo:warning=upstream wrapper not found at {}; skipping dev wrapper bundling",
            source.display()
        );
        return;
    }

    install_wrapper(&source, &dest);

    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("yt-dlp.sh").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("yt-dlp.cmd").display()
    );
}

#[cfg(unix)]
fn install_wrapper(source: &Path, dest: &Path) {
    use std::fs;

    if dest.is_symlink() {
        // canonicalize handles the macOS /var ↔ /private/var aliasing case;
        // if either canonicalize fails (filesystem error), we err on the side
        // of re-creating the symlink — the cost is negligible.
        let source_canon = source.canonicalize().ok();
        let dest_target_canon = dest.read_link().ok().and_then(|t| {
            // read_link can return a relative path; resolve it against the
            // dest's parent before canonicalizing.
            let resolved = if t.is_absolute() {
                t
            } else {
                dest.parent().map_or(t.clone(), |p| p.join(t))
            };
            resolved.canonicalize().ok()
        });
        if source_canon.is_some() && source_canon == dest_target_canon {
            return;
        }
    }

    if (dest.exists() || dest.is_symlink())
        && let Err(err) = fs::remove_file(dest)
    {
        println!(
            "cargo:warning=failed to remove existing wrapper at {}: {err}; skipping dev wrapper bundling",
            dest.display()
        );
        return;
    }

    if let Err(err) = std::os::unix::fs::symlink(source, dest) {
        println!(
            "cargo:warning=failed to symlink {} -> {}: {err}; skipping dev wrapper bundling",
            dest.display(),
            source.display()
        );
    }
}

#[cfg(windows)]
fn install_wrapper(source: &Path, dest: &Path) {
    use std::fs;

    // some filesystems have second-resolution mtime granularity, which can
    // produce a false-no-op for sub-second changes; for UC 03's read-only
    // upstream wrapper this is fine.
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
                return;
            }
        }
    }

    if let Err(err) = fs::copy(source, dest) {
        println!(
            "cargo:warning=failed to copy {} -> {}: {err}; skipping dev wrapper bundling",
            source.display(),
            dest.display()
        );
    }
}

/// Walks up from `start` until it finds an ancestor whose `Cargo.toml`
/// contains the literal substring `[workspace]`. Returns `None` if no such
/// ancestor exists.
fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        let manifest = ancestor.join("Cargo.toml");
        if let Ok(contents) = std::fs::read_to_string(&manifest)
            && contents.contains("[workspace]")
        {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

// CONVENTION: Cargo places build-script outputs at
//   target/<...>/<profile>/build/<crate>-<hash>/out
// We climb four ancestors of OUT_DIR (out -> <crate>-<hash> -> build -> <profile>)
// to reach the profile dir. This is Cargo convention, not a stable contract;
// a future Cargo layout change would silently misroute bundling.
// The unit tests in crates/app/src/build_paths_test.rs lock the parser to
// the documented layout.
fn profile_dir_from_out_dir(out_dir: &Path) -> Option<PathBuf> {
    let crate_hash_dir = out_dir.parent()?;
    let build_dir = crate_hash_dir.parent()?;
    let profile_dir = build_dir.parent()?;
    if build_dir.file_name().and_then(|n| n.to_str()) != Some("build") {
        return None;
    }
    Some(profile_dir.to_path_buf())
}

/// UC 17: copies the dev `runtime-deps/ffmpeg` next to the cargo-produced
/// `app` binary so `cargo run` can invoke yt-dlp with `--ffmpeg-location`
/// without a system-wide install.
///
/// Behaviour:
/// - Skipped under cargo-dist (release pipeline produces the bundled binary
///   via the package-* workflows; this script is only for the dev profile).
/// - Skipped when `YT_DLP_UI_FETCH_DEPS=0` is set (opt-out for offline /
///   air-gapped builds — see CONTRIBUTING.md).
/// - When `runtime-deps/ffmpeg` is missing, the function attempts to fetch
///   it via `scripts/fetch-ffmpeg.sh` (Unix) on the host's target triple.
///   Any fetch failure is reported via `cargo:warning=` and the build
///   continues — the spawn-time gate in the download manager will surface
///   the missing-binary state to the user at runtime.
/// - The binary is COPIED (not symlinked) into `<profile_dir>/ffmpeg` so
///   later changes to `runtime-deps/ffmpeg` are picked up by the next build.
fn bundle_dev_ffmpeg() {
    println!("cargo:rerun-if-env-changed=CARGO_DIST_TARGET");
    println!("cargo:rerun-if-env-changed=DIST_TARGET");
    println!("cargo:rerun-if-env-changed=YT_DLP_UI_FETCH_DEPS");
    println!("cargo:rerun-if-env-changed=PROFILE");

    if std::env::var_os("CARGO_DIST_TARGET").is_some() || std::env::var_os("DIST_TARGET").is_some()
    {
        return;
    }

    let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from) else {
        println!("cargo:warning=CARGO_MANIFEST_DIR not set; skipping dev ffmpeg bundling");
        return;
    };
    let Some(workspace_root) = find_workspace_root(&manifest_dir) else {
        return;
    };
    let Some(out_dir) = std::env::var_os("OUT_DIR").map(PathBuf::from) else {
        return;
    };
    let Some(profile_dir) = profile_dir_from_out_dir(&out_dir) else {
        return;
    };

    let runtime_deps = workspace_root.join("runtime-deps");
    let bin_name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let source = runtime_deps.join("ffmpeg");
    let dest = profile_dir.join(bin_name);

    let fetch_allowed = std::env::var("YT_DLP_UI_FETCH_DEPS").map_or(true, |v| v != "0");

    if !source.is_file() {
        if !fetch_allowed {
            println!(
                "cargo:warning=runtime-deps/ffmpeg missing and YT_DLP_UI_FETCH_DEPS=0; downloads requiring ffmpeg will error"
            );
            return;
        }
        try_fetch_dev_ffmpeg(&workspace_root);
        if !source.is_file() {
            println!(
                "cargo:warning=runtime-deps/ffmpeg unavailable after fetch attempt; downloads requiring ffmpeg will error"
            );
            return;
        }
    }

    // Copy-not-symlink so a `runtime-deps/ffmpeg` rotation reaches the
    // profile dir on the next build without a stale-symlink hazard.
    if let Err(err) = std::fs::copy(&source, &dest) {
        println!(
            "cargo:warning=failed to copy {} -> {}: {err}",
            source.display(),
            dest.display()
        );
        return;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&dest) {
            let mut perm = meta.permissions();
            perm.set_mode(0o755);
            let _ = std::fs::set_permissions(&dest, perm);
        }
    }

    println!("cargo:rerun-if-changed={}", source.display());
}

/// Best-effort `scripts/fetch-ffmpeg.sh` invocation on Unix dev hosts.
/// Errors are surfaced as `cargo:warning=` and never panic the build.
#[cfg(unix)]
fn try_fetch_dev_ffmpeg(workspace_root: &Path) {
    let script = workspace_root.join("scripts").join("fetch-ffmpeg.sh");
    if !script.is_file() {
        println!(
            "cargo:warning=fetch script not found at {}; cannot auto-fetch dev ffmpeg",
            script.display()
        );
        return;
    }

    // macOS dev hosts cannot use the BtbN/FFmpeg-Builds path (no LGPL-only
    // mainstream macOS build); the fetch script hard-fails on darwin. The
    // dev-host workflow on macOS is `bash scripts/build-ffmpeg-macos.sh
    // runtime-deps/`, documented in CONTRIBUTING.md and exercised by
    // `just fetch-runtime-deps`.
    if cfg!(target_os = "macos") {
        return;
    }

    let Ok(target) = std::env::var("TARGET") else {
        return;
    };
    let runtime_deps = workspace_root.join("runtime-deps");
    let _ = std::fs::create_dir_all(&runtime_deps);

    let status = std::process::Command::new("bash")
        .arg(&script)
        .arg(&target)
        .arg(&runtime_deps)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            println!(
                "cargo:warning=fetch-ffmpeg.sh exited with status {s}; downloads requiring ffmpeg may error"
            );
        }
        Err(err) => {
            println!("cargo:warning=failed to invoke fetch-ffmpeg.sh: {err}");
        }
    }
}

#[cfg(not(unix))]
fn try_fetch_dev_ffmpeg(_workspace_root: &Path) {
    // Windows dev hosts run the .ps1 fetch via `just fetch-runtime-deps`
    // explicitly; auto-invoking PowerShell from build.rs is too fragile
    // (execution policy, profile loading). Document and step out.
    println!(
        "cargo:warning=auto-fetch of ffmpeg is Unix-only; on Windows run `just fetch-runtime-deps` manually"
    );
}

/// UC 18: parse `scripts/runtime-deps-pins.env` and bake the pinned versions
/// into the binary as compile-time `cargo:rustc-env=` strings consumed by
/// `crates/app/src/about.rs`. Failure to read or parse the file is reported
/// via `cargo:warning=` and the missing keys are baked as `unknown` so the
/// build never panics.
fn bake_runtime_pins() {
    println!("cargo:rerun-if-env-changed=YT_DLP_UI_FORCE_RERUN_PINS");

    let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from) else {
        emit_pin_unknown();
        println!("cargo:warning=CARGO_MANIFEST_DIR not set; baking pins as 'unknown'");
        return;
    };

    let Some(workspace_root) = find_workspace_root(&manifest_dir) else {
        emit_pin_unknown();
        println!("cargo:warning=workspace root not found; baking pins as 'unknown'");
        return;
    };

    let pins_path = workspace_root.join("scripts").join("runtime-deps-pins.env");

    println!("cargo:rerun-if-changed={}", pins_path.display());

    let contents = match std::fs::read_to_string(&pins_path) {
        Ok(s) => s,
        Err(err) => {
            emit_pin_unknown();
            println!(
                "cargo:warning=failed to read {}: {err}; baking pins as 'unknown'",
                pins_path.display()
            );
            return;
        }
    };

    let pins = parse_pins(&contents);

    let yt_dlp = pins.get("YT_DLP_VERSION").map_or("unknown", String::as_str);
    let deno = pins.get("DENO_VERSION").map_or("unknown", String::as_str);
    let ffmpeg_tag = pins
        .get("FFMPEG_RELEASE_TAG")
        .map_or("unknown", String::as_str);
    let ffmpeg_source = pins
        .get("FFMPEG_VERSION_SOURCE")
        .map_or("unknown", String::as_str);

    println!("cargo:rustc-env=YT_DLP_UI_PINNED_YT_DLP_VERSION={yt_dlp}");
    println!("cargo:rustc-env=YT_DLP_UI_PINNED_DENO_VERSION={deno}");
    println!("cargo:rustc-env=YT_DLP_UI_PINNED_FFMPEG_RELEASE_TAG={ffmpeg_tag}");
    println!("cargo:rustc-env=YT_DLP_UI_PINNED_FFMPEG_VERSION_SOURCE={ffmpeg_source}");

    // UC 18: warn until the LGPL-2.1 placeholder is replaced. We probe by
    // size — the placeholder file ships at <200 bytes; the canonical text
    // is ~25 KB. Any non-trivial replacement clears the warning. The
    // `rerun-if-changed` directive ensures the warning state tracks the
    // file content rather than getting cached on the first build.
    let lgpl_path = manifest_dir
        .join("assets")
        .join("licenses")
        .join("lgpl-2.1.txt");
    println!("cargo:rerun-if-changed={}", lgpl_path.display());
    if let Ok(meta) = std::fs::metadata(&lgpl_path)
        && meta.len() < 1024
    {
        println!(
            "cargo:warning=licenses/lgpl-2.1.txt is a TODO placeholder; replace with canonical text before release"
        );
    }
}

/// Bakes all four pin env vars as `unknown`. Used when we cannot read the
/// pins file at all (no manifest dir, no workspace root, IO error). Keeps
/// `env!` lookups in `about.rs` valid so the build never fails.
fn emit_pin_unknown() {
    println!("cargo:rustc-env=YT_DLP_UI_PINNED_YT_DLP_VERSION=unknown");
    println!("cargo:rustc-env=YT_DLP_UI_PINNED_DENO_VERSION=unknown");
    println!("cargo:rustc-env=YT_DLP_UI_PINNED_FFMPEG_RELEASE_TAG=unknown");
    println!("cargo:rustc-env=YT_DLP_UI_PINNED_FFMPEG_VERSION_SOURCE=unknown");
}

/// Pure-stdlib parser for `scripts/runtime-deps-pins.env`. Skips blank
/// lines and `#`-prefixed comments (after trimming), tolerates trailing
/// CR (`\r\n` line endings), splits on the first `=`, trims both sides.
/// Later occurrences of the same key win.
fn parse_pins(contents: &str) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for raw in contents.lines() {
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if !key.is_empty() {
                out.insert(key.to_string(), value.to_string());
            }
        }
    }
    out
}
