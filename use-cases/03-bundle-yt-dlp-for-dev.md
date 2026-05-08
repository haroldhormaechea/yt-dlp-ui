# Use Case 03: Auto-bundle yt-dlp for dev workflow

## Summary

Extend `crates/app/build.rs` so the upstream `yt-dlp` wrapper script is automatically placed next to the compiled `app` binary at `cargo run`-time, eliminating the need for developers to install or symlink yt-dlp manually. The wrappers `<workspace_root>/yt-dlp.sh` (Unix) and `<workspace_root>/yt-dlp.cmd` (Windows) already exist in the repo and invoke the upstream `yt_dlp` Python module that lives at `<workspace_root>/yt_dlp/`. The build script computes the cargo profile output directory from `OUT_DIR` and creates a symlink on Unix or a file copy on Windows so the wrapper sits at `<target_profile_dir>/yt-dlp` (or `yt-dlp.cmd` on Windows), where `bundled_yt_dlp_path()`'s "next-to-binary" debug fallback already looks. On Windows, `bundled_yt_dlp_path()` is extended (debug builds only) to accept `yt-dlp.cmd` alongside `yt-dlp.exe`. The release-build path (which fetches and SHA-verifies the upstream standalone binary per `PROJECT_BRIEF.md` § Deployment) is intentionally unchanged — UC 03's scope is dev workflow only. After this UC, `cargo run --bin app` and `cargo run --release --bin app` succeed on a fresh clone without a separate yt-dlp install. README.UI.md is updated to retract the prior incorrect claim that no yt-dlp install is needed and to document Python 3 as the only remaining dev prerequisite.

## Acceptance Criteria

1. `cargo run --bin app` succeeds on a fresh clone (no manual yt-dlp install, no manual symlink) on macOS, Linux, and Windows hosts that have Python 3 available. The same holds for `cargo run --release --bin app`.
2. `crates/app/build.rs` runs the new bundling step after the existing slint compile step. On Unix hosts, it creates a symlink at `<target_profile_dir>/yt-dlp` pointing to `<workspace_root>/yt-dlp.sh`. On Windows hosts, it copies `<workspace_root>/yt-dlp.cmd` to `<target_profile_dir>/yt-dlp.cmd`.
3. The bundling step is idempotent: on subsequent builds where the symlink (Unix) or copy (Windows) already exists and points to / matches the correct source, the step is a no-op (no unnecessary writes, no rebuild churn).
4. `<target_profile_dir>` is computed from `OUT_DIR` (climbing `OUT_DIR/build/<crate>-<hash>/out` back to the cargo profile dir) — not hardcoded as `target/debug`. The implementation works for `cargo build`, `cargo build --release`, and `cargo build --target <triple>`.
5. `cargo:rerun-if-changed` directives are emitted for `<workspace_root>/yt-dlp.sh` and `<workspace_root>/yt-dlp.cmd` so changes to the wrappers trigger re-bundling.
6. On Windows, `crates/app/src/paths.rs::bundled_yt_dlp_path()` is extended (debug builds only, gated by `cfg!(debug_assertions)`) to also check for `yt-dlp.cmd` next to the binary if `yt-dlp.exe` is not found. Release builds still expect `yt-dlp.exe` only.
7. The existing PATH-scan dev fallback in `bundled_yt_dlp_path()` (for cases where the bundled binary is missing) is preserved — UC 03 adds a new bundling path, it does not remove the existing fallback.
8. Release builds are NOT changed by this UC. The release pipeline's bundling (per `PROJECT_BRIEF.md` § Deployment) remains the future deployment-channel work item.
9. The UC introduces NO new third-party Cargo dependencies. The build script uses only `std` (`std::fs`, `std::os::unix::fs::symlink` on Unix, `std::fs::copy` on Windows, `std::env`, `std::path::PathBuf`).
10. `README.UI.md` is updated as part of this UC:
    - Remove the inaccurate Requirements line "The bundled `yt-dlp` binary is fetched at release-build time; you do not need a separate `yt-dlp` install on PATH for development."
    - Add **Python 3** to Requirements as the only remaining dev prerequisite (the wrapper invokes Python on the upstream module).
    - Add a Requirements note that on a fresh `rustup` install, the user may need to `source "$HOME/.cargo/env"` (or open a new shell) before `cargo` is on PATH — this is rustup default behavior, not a project bug, but it bites first-run users.
    - Quick start gains a plain-cargo path (`cargo run --release --bin app`) alongside the existing `just` recipes.
    - Remove the stale "There is no usable UI yet — the binaries currently log a startup line and exit" sentence (UC 01 has shipped).
11. Tests cover:
    - The build script's path-resolution logic for `<target_profile_dir>` (unit-testable as a pure helper that takes a hypothetical `OUT_DIR` string and returns the resolved profile path).
    - The Windows `yt-dlp.cmd` fallback in `bundled_yt_dlp_path()` for debug builds (extends the existing `paths_test.rs`).
    - The bundling step's idempotency (running the helper twice in a row in a tempdir produces identical filesystem state).

## Potential Pitfalls & Open Questions

- **Risk** — Symlinks on Windows require admin rights or Developer Mode. We deliberately avoid `std::os::windows::fs::symlink_file` and use `std::fs::copy` on Windows for that reason. The copy is checked-and-skipped when the destination already matches the source (mtime + length) to keep idempotency.
- **Risk** — `OUT_DIR` location (`target/<profile>/build/<crate>-<hash>/out`) is a Cargo convention, not a stable contract. If Cargo changes the layout, the climb-back breaks. Mitigation: comment the assumption in the build script and add a unit test asserting the climb-back logic against the documented layout.
- **Edge case** — `cargo build --target <triple>` puts artifacts in `target/<triple>/<profile>/`. The climb-back from `OUT_DIR` (`target/<triple>/<profile>/build/<crate>-<hash>/out`) still resolves correctly to `target/<triple>/<profile>/`, but the developer should add an explicit test case for this.
- **Risk** — Some dev environments don't have Python 3 in PATH. The wrapper's first line is `#!/usr/bin/env sh\nexec "${PYTHON:-python3}" …`; if `python3` is not found, the app fails at first yt-dlp invocation with a confusing error. Mitigation: README change in AC#10 lists Python 3 as a requirement. Optional follow-up: have `bundled_yt_dlp_path()` probe the wrapper at startup and surface a clear error if Python is missing — not in scope for UC 03.
- **Edge case** — `cargo clean` removes everything under `target/`, including the symlink/copy. Next `cargo run` re-runs `build.rs` and re-bundles, so the situation self-heals. Verify in a smoke test.
- **Edge case** — The workspace root from a build script is reachable via `CARGO_MANIFEST_DIR/../..` (since the manifest dir is `crates/app/`). Hardcoding `../../` is fragile if the workspace structure changes; use `Path::ancestors()` and stop at the first dir that contains `Cargo.toml` with `[workspace]`. Or trust the convention and document it.
- **Risk** — On macOS, the dev fallback chain in `bundled_yt_dlp_path()` is: per-OS bundled path (`<binary>/../Resources/yt-dlp` for an `.app` bundle, doesn't exist in cargo dev) → next-to-binary (`<target_profile_dir>/yt-dlp`, NEW from this UC) → PATH scan. Verify the order remains correct via a paths_test that constructs a tempdir mirroring each layer.
- **Edge case** — Windows users running `cargo run` from a non-elevated shell will hit the symlink-permission issue if we ever fall back to symlinking. The decision to use `fs::copy` on Windows is what avoids this — keep it.
- **Assumption** — Production code change for Windows (AC#6) is bounded to a single `cfg(windows)` `cfg(debug_assertions)` block in `paths.rs` and does not affect release builds. The challenger should confirm this is faithful to the brief's "release-only path" guarantee.

## Original Description

> We need a way of testing dev while also bundling yt-dlp so you don't need to also install it separately, considering that we are supposed to be bundling it.
>
> Resolution after team-lead/user discussion:
> Option 1 — Build script auto-symlinks the upstream wrapper. The repo already has yt-dlp.sh / yt-dlp.cmd, both of which invoke the upstream Python yt_dlp module that lives in the repo. The build script symlinks (Unix) or copies (Windows) the wrapper next to the cargo-produced app binary on every build. On Windows, paths.rs::bundled_yt_dlp_path() is extended in debug-only mode to also accept yt-dlp.cmd. README.UI.md is corrected (the prior "no separate yt-dlp install needed" claim was wrong) and the rustup PATH gotcha is documented. Release-build bundling (download + verify the upstream standalone binary) is unchanged by this UC.

## Clarifications

- Q: Auto-bundle via build script (Option 1), download upstream binary in build script (Option 2), or document a manual symlink (Option 3)?
  A: Option 1. Cleanest dev ergonomics, no network, leverages the upstream Python source already in the repo. Release-pipeline bundling (Option 2-style fetch + SHA verify) remains a separate deployment-channel work item.
- Q: Run this directly as orchestrator-level cleanup or kick it through the dev-team as a small UC?
  A: Through the dev-team. The Windows `paths.rs` change is real production code; the build-script change deserves a peer-reviewed proposal even though it's small.
