# 0005 — yt-dlp bundling

- **Status:** accepted
- **Date:** 2026-04-25

## Context

The UI wraps the existing `yt-dlp` CLI. At runtime we need a `yt-dlp`
binary to invoke. Three plausible strategies:

- **(a) Bundle the binary in each installer.** Pinned upstream version,
  determinism, no PATH surprises.
- **(b) Expect `yt-dlp` on PATH.** Smallest install, but non-technical
  users will not have it.
- **(c) Bundle + auto-update yt-dlp at runtime.** Bundle initially, then
  check for newer upstream releases on app start.

The target audience is non-technical, ruling out (b). In-app self-update
of yt-dlp was descoped from MVP, ruling out (c).

A separate question: **what** to bundle? Two options upstream produces:
- The Python `yt_dlp` package (requires shipping a Python interpreter).
- The standalone single-file executable produced by upstream's
  PyInstaller-based bundler (one self-contained binary).

## Decision

**Bundle the upstream standalone single-file executable** in each
installer. This is what upstream officially distributes (`yt-dlp` for
Linux, `yt-dlp_macos` for macOS, `yt-dlp.exe` for Windows). The Python
source tree under `yt_dlp/` is **never invoked at runtime** and is **never
modified at the source level**.

The `yt-dlp-bridge` crate invokes the bundled binary via
`tokio::process::Command`. The path to the binary is per-OS and resolved
relative to `app`'s own executable (PROJECT_BRIEF.md § Architecture
§ Bundled-binary path). The bridge accepts the path as a constructor
argument so it can be unit-tested with a fake binary.

The release pipeline downloads the upstream binary at build time, verifies
upstream's GPG signature using the existing `scripts/keys/yt-dlp.asc` key
file, and verifies the SHA256 hash against upstream's `SHA2-256SUMS`.

## Consequences

**Positive:**
- Deterministic runtime: every installer ships a known yt-dlp version.
- No Python runtime in the installer; smaller bundle.
- Read-only-upstream-tree rule is preserved at the source level. The
  Python tree exists in the repo (we forked with history) but is not
  invoked, modified, or copied into installers.
- Supply-chain story is clear: GPG + SHA256 verification of the upstream
  artifact.

**Negative:**
- Bundle size grows by ~15 MB per OS for the bundled `yt-dlp`. Acceptable
  within the < 50 MB compressed installer budget.
- Bumping the bundled yt-dlp version is a manual PR (changing
  `YT_DLP_VERSION` in CI). Auto-bumping is deferred.
- If the user wants a newer yt-dlp than the bundled version, they must
  wait for the next release of `yt-dlp-ui` or manually swap the binary
  inside the installed bundle.

## Alternatives considered

- **Expect on PATH** — fails the non-technical audience.
- **Bundle Python + yt_dlp package** — adds Python runtime weight and a
  much larger attack surface. Worse trust story.
- **Bundle + auto-update at runtime** — descoped from MVP.

## References

- PROJECT_BRIEF.md § Technologies § yt-dlp invocation
- PROJECT_BRIEF.md § Deployment § yt-dlp binary supply chain
- THREATS.md § T4 (supply-chain risk)
