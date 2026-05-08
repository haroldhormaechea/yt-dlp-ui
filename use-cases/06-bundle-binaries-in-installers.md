# Use Case 06: Bundle yt-dlp and deno into release installers

## Summary

Wire the GitHub Actions release workflow to fetch and bundle the upstream yt-dlp binary (SHA256 + GPG verification per `PROJECT_BRIEF.md` § D6) AND the deno binary (SHA256 only, mirroring UC 05's posture) into the cargo-dist-produced installer artifacts (.dmg / NSIS .exe / .deb / .rpm) for every target triple. After this UC, an end user installing yt-dlp-ui via .dmg / .exe / .deb / .rpm gets a fully working app — no `brew install yt-dlp`, no `brew install deno`, zero prerequisites. Mirrors UC 05's `scripts/fetch-deno.sh` pattern: a new `scripts/fetch-yt-dlp.sh` sits alongside it; PowerShell parallels (`fetch-yt-dlp.ps1` + `fetch-deno.ps1`) cover Windows runners with native tooling (no Git Bash dependency). A new pre-build matrix step in `.github/workflows/release.yml` invokes the OS-appropriate script per target triple, places binaries at the per-OS bundled path expected by `crates/app/src/paths.rs::bundled_yt_dlp_path()` / `bundled_deno_path()`. cargo-dist's installer-payload config carries them into each artifact. QA verifies the result via a local `dist build --artifacts=local` smoke on the macOS host before marking done. The `.snap` path remains a separate concern (UC 07 candidate). Dev mode (UC 03 wrapper symlink + UC 05 deno startup probe + warning banner) is unchanged.

## Acceptance Criteria

1. NEW `scripts/fetch-yt-dlp.sh` mirrors `scripts/fetch-deno.sh`'s argv shape: `<target-triple> <output-dir>`. Reads `YT_DLP_VERSION` env (fail if unset). Curl-fetches the upstream release asset matching the target triple from `https://github.com/yt-dlp/yt-dlp/releases/download/<YT_DLP_VERSION>/`. Verifies SHA256 against upstream's `SHA2-256SUMS` AND GPG-verifies against a temporary keyring loaded from the in-repo `public.key`. Places the binary at `<output-dir>/yt-dlp` (or `yt-dlp.exe`). `chmod +x` on Unix.

2. NEW `scripts/fetch-yt-dlp.ps1` — PowerShell parallel of `fetch-yt-dlp.sh`. Same argv contract. SHA256 + GPG verification using the same `public.key` and the same upstream `SHA2-256SUMS` source. Used by Windows GHA runners.

3. NEW `scripts/fetch-deno.ps1` — PowerShell parallel of UC 05's existing `fetch-deno.sh`. Same argv contract. SHA256-only verification (deno does not publish GPG signatures; THREATS.md T11 documents this asymmetry). Used by Windows GHA runners.

4. `.github/workflows/release.yml` gains a per-target-triple pre-build step. The step matrix dispatches by `runner.os`:
   - **`Linux`, `macOS` runners** → invoke `scripts/fetch-yt-dlp.sh` and `scripts/fetch-deno.sh`.
   - **`Windows` runners** → invoke `scripts/fetch-yt-dlp.ps1` and `scripts/fetch-deno.ps1`.
   The step runs BEFORE cargo-dist's build step so the binaries are in place when cargo-dist packages.

5. cargo-dist's installer-payload config (`dist-workspace.toml` or per-package `[package.metadata.dist]`) is updated so each installer artifact carries both binaries at the correct per-OS bundled path:
   - **macOS .dmg:** `<App>.app/Contents/Resources/yt-dlp` and `<App>.app/Contents/Resources/deno`.
   - **Windows NSIS .exe:** `<install-dir>\yt-dlp.exe` and `<install-dir>\deno.exe` next to `yt-dlp-ui.exe`.
   - **Linux .deb / .rpm:** at the same install prefix as the main binary (e.g. `/usr/lib/yt-dlp-ui/yt-dlp` and `/usr/lib/yt-dlp-ui/deno`, or wherever cargo-dist's Linux pkg layout puts companion binaries).
   The exact mechanism (cargo-dist `extra_artifacts` / `include` / a per-format hook) is the developer's choice — whichever cargo-dist 0.31.0 supports cleanly per target.

6. `crates/app/src/paths.rs::bundled_yt_dlp_path()` and `bundled_deno_path()` continue to resolve correctly against the bundled artifact. No production code change is expected; if the workflow's chosen install path doesn't match the existing `paths.rs` resolution, prefer adjusting the workflow over changing `paths.rs` (paths.rs is shared with UC 03's dev-mode resolution).

7. **QA done-criteria includes a local `dist build --artifacts=local` smoke** on the macOS host: QA runs the command, opens the resulting `.dmg` (or extracts the `.tar.xz` if the macOS .dmg path requires signing tooling QA doesn't have), and confirms `yt-dlp` + `deno` are both present at the expected `<App>.app/Contents/Resources/` paths and are executable (`-x` permission, `file <path>` shows valid Mach-O / arch-correct binary). The smoke is documented in QA's final summary; failure of this check blocks Task #3 completion.

8. Dev mode is unchanged. `cargo run --bin app` from the repo continues to use:
   - UC 03's `target/<profile>/yt-dlp` symlink to the in-repo `yt-dlp.sh` wrapper.
   - UC 05's deno startup probe (PATH lookup) + dismissible warning banner when deno is missing.
   The release-mode bundling is additive; it does not displace dev-mode behavior.

9. `.snap` artifact bundling is explicitly OUT of UC 06 scope. The `.snap` gap (cargo-dist does not generate snap artifacts; needs `snapcore/action-build` + `snapcore/action-publish` + Snap Store account + `snapcraft.yaml`) remains a known scaffolding gap (per `PROJECT_BRIEF.md` § Scaffolding Plan). Capture as a UC 07 candidate.

10. `YT_DLP_VERSION` and `DENO_VERSION` are pinned in `.github/workflows/release.yml` as workflow-level env vars. Bumping requires editing the workflow and opening a reviewed PR. Document the bump procedure briefly in `README.UI.md` (or a new `RELEASE.md` if the dev-team judges that cleaner).

11. New tests:
    - Shell-test for `scripts/fetch-yt-dlp.sh` with bash: argv parsing (missing args → fail), missing `YT_DLP_VERSION` env → fail with clear message, target-triple → asset-name mapping correctness, SHA mismatch path → script exits non-zero (use a tampered local fixture), GPG-failure path → script exits non-zero (use an unsigned fixture).
    - Smoke-level argv parsing test for both `.ps1` scripts: invoke with bad args, expect non-zero exit + clear error. Full SHA/GPG paths in PowerShell are not unit-testable cheaply; rely on the local `dist build` smoke (AC#7) for end-to-end validation on Windows.
    - `actionlint` over the new workflow file (already part of the existing `test-workflows.yml` job per UC 03's resolution; confirm it covers the new step).

12. `README.UI.md` Requirements section is updated:
    - **End users**: remove the Deno install instruction (no longer required after install).
    - **Developers**: keep the Deno install instruction (dev mode still requires deno on PATH for full YouTube format extraction).
    - The end-user vs developer split is made explicit in the section header or sub-bullets.

13. `PROJECT_BRIEF.md` § Deployment is amended to record that UC 06 closed the yt-dlp + deno bundling gap. The remaining open release-pipeline items (`.snap` workflow, Posture 3 → Posture 1 signed-binaries revisit) stay flagged for future work.

14. All three gates pass: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`. No Rust source changes are anticipated; the gates run for regression safety.

## Potential Pitfalls & Open Questions

- **Risk** — `.sh` + `.ps1` divergence: every fix or version-bump must land in both shells. Document the dual-script discipline in `CONTRIBUTING.UI.md` or a new `scripts/README.md`. Consider a CI step that diff-checks the two scripts' argv contract and version-string anchors to catch drift early.
- **Risk** — cargo-dist installer-payload mechanism: not all formats accept arbitrary extra files cleanly. The dev-team analyst should verify each target's mechanism in cargo-dist 0.31.0 docs / experimentation BEFORE coding the workflow step. Possible fallbacks per target if cargo-dist's native mechanism isn't sufficient: (a) a custom post-build hook that injects the binaries, (b) bundling via a per-target `Cargo.toml [[package.metadata.dist.extra]]` block.
- **Edge case** — yt-dlp release-asset naming variance: `yt-dlp_linux` (Linux x86_64), `yt-dlp_linux_aarch64` (Linux aarch64), `yt-dlp_macos`, `yt-dlp.exe` (Windows). Deno uses target-triple tarballs (`deno-aarch64-apple-darwin.zip`, etc.). Both fetch scripts × both shells must handle each pattern correctly.
- **Risk** — Bundled-binary chmod +x on Unix: cargo-dist's payload mechanism may or may not preserve POSIX permissions. If not, the fetch script's `chmod +x` is sufficient (the binary is executable in `<output-dir>` BEFORE cargo-dist picks it up); if cargo-dist re-zips and strips permissions, the workflow needs an explicit post-build chmod step.
- **Risk** — GPG keyring import in CI: `fetch-yt-dlp.sh` must `gpg --import public.key` into a temp keyring (`GNUPGHOME=$(mktemp -d)`) before `gpg --verify` to avoid polluting the runner's default keyring. Same for the .ps1 (using `gpg.exe` from the GPG4Win install on Windows runners — verify its presence). Document keyring-cleanup at end of script.
- **Edge case** — Cross-arch fetching on GHA: when `ubuntu-latest` (x86_64) targets `aarch64-unknown-linux-gnu`, the script fetches the aarch64-Linux binary. The fetch is platform-independent (just curl + verify); only the `chmod +x` and final placement run on the host runner.
- **Risk** — Posture 3 (unsigned binaries) is unchanged by UC 06. macOS Gatekeeper will warn for the whole installer (the .dmg + the embedded .app). Bundling yt-dlp / deno inside doesn't change the warning posture; it merely ensures they're present once the user bypasses Gatekeeper. Documented under existing Known Limitations.
- **Edge case** — UC 05's deno startup banner remains as a safety net: if the bundled deno is somehow missing at runtime (damaged install, manual deletion), the banner reminds the user to install. Banner text is already correct ("Some YouTube downloads may require Deno; install via brew install deno…"); UC 06 does not change it.
- **Risk** — `dist build --artifacts=local` (AC#7) on macOS may invoke `create-dmg` or similar tooling that QA may not have installed. Fallback: extract the resulting `.tar.xz` and inspect `<App>.app/Contents/Resources/` directly. Document this fallback in QA's prompt.
- **Edge case** — `YT_DLP_VERSION` bump procedure: the dev-team should pin a concrete value at UC 06 implementation time (e.g. the latest stable yt-dlp at that moment) and document the bump procedure as a one-line step (edit workflow, PR, review). Don't auto-bump in this UC.

## Original Description

> "I want to make it so anyone installing this gets deno automatically pulled, we can't ask them for prerequisites!"

Context: today's release-mode gaps are (a) deno is not bundled into installers despite UC 05 shipping the callable `scripts/fetch-deno.sh` hook, and (b) yt-dlp release-time fetching was specified in `PROJECT_BRIEF.md` § D6 but never built — UC 03 only handled dev-mode bundling via the in-repo `yt-dlp.sh` wrapper. Both binaries must be inside installers for the user's "no prerequisites" promise to hold.

## Clarifications

- Q: yt-dlp verification — SHA + GPG (per brief § D6) or SHA-only across the board?
  A: yt-dlp = SHA + GPG, deno = SHA-only. Each binary uses the strongest verification its publisher provides. Deno's SHA-only-vs-GPG asymmetry is documented in THREATS.md T11.
- Q: `.snap` artifact bundling — include in UC 06 or split?
  A: Split to a separate UC. UC 06 covers .dmg / NSIS / .deb / .rpm via cargo-dist's one mechanism. Snap requires a fundamentally different workflow + Snap Store account + snapcraft.yaml + Canonical's manual review.
- Q: Windows runner shell — Git Bash, .ps1 parallels, or WSL?
  A: PowerShell parallels (`.ps1`) for Windows. Native tooling, no Git Bash dependency. Acknowledged tradeoff: .sh + .ps1 must be kept in sync.
- Q: QA done-criteria — require local `dist build --artifacts=local` smoke?
  A: Required. QA runs it on macOS, verifies yt-dlp + deno are at the expected bundled paths in the resulting artifact. Catches workflow / cargo-dist config issues before the next real release.
