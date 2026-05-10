# Use Case 25: Trim release-pipeline assets to installers + provenance only

## Summary

Trim the GitHub release asset set produced by the tag release pipeline so only end-user-facing artifacts and a minimal provenance set are attached. Today every published release carries 20 cargo-dist intermediate per-arch archives (`app-*.tar.xz` / `app-*-pc-windows-msvc.zip`, the `ad-window-*` counterparts, plus `.sha256` sidecars) because cargo-dist's default workspace config publishes them. These archives are only meaningful as inputs to the custom `package-deb-rpm`, `package-dmg`, and `package-nsis` jobs, which already consume them as workflow artifacts via `actions/download-artifact`. The implementer should first try a cargo-dist config tweak in `dist-workspace.toml` to mark the archives as build-only / non-publishable; if no clean switch exists in the installed cargo-dist version, fall back to a `gh release delete-asset` post-process step in `release.yml`'s `host` job. Per-arch archives must remain available as workflow artifacts so the packaging chain continues to function. `dist-manifest.json` is also dropped from the public release. The change applies to future tags only — existing releases are not touched.

## Acceptance Criteria

1. After a fresh tag push (e.g. `v0.5.1-rc.7`), the GitHub release page lists exactly these assets and no others:
   - `yt-dlp-ui-installer.exe`
   - `yt-dlp-ui-universal.dmg`
   - `yt-dlp-ui_<ver>_amd64.deb`, `yt-dlp-ui_<ver>_arm64.deb`
   - `yt-dlp-ui-<ver>-1.x86_64.rpm`, `yt-dlp-ui-<ver>-1.aarch64.rpm`
   - `sha256.sum`
   - `source.tar.gz`, `source.tar.gz.sha256`
2. None of `app-*.tar.xz`, `app-*.zip`, `ad-window-*.tar.xz`, `ad-window-*.zip`, their `.sha256` sidecars, nor `dist-manifest.json` appear on the release page.
3. `custom-package-nsis`, `custom-package-dmg`, and both `custom-package-deb-rpm` jobs still pass — they download the per-arch archives via `actions/download-artifact` and produce installers exactly as before.
4. The five `build-local-artifacts` jobs continue to upload per-arch archives as **workflow** artifacts (visible in the Actions UI for debugging; not on the release).
5. `host` and `announce` complete successfully end-to-end; the release retains its prerelease flag and title.
6. The implementer first attempts a `dist-workspace.toml` config change. If cargo-dist exposes no clean knob (documented in the PR description), they fall back to a `gh release delete-asset` step inside `release.yml`'s `host` job, ordered before `announce` so the bloated state is never publicly observable.
7. The chosen mechanism is documented inline (config comment or workflow comment) so future contributors understand why.
8. Pre-existing releases (`v0.5.0`, all `v0.5.1-rc.*`) are not modified — the trim applies only to future tags.

## Potential Pitfalls & Open Questions

- **Risk** — cargo-dist's `dist = true` on binary crates may be load-bearing in ways beyond release publishing (e.g., manifest generation, install scripts). If no clean "build but don't publish" switch exists in the installed cargo-dist version, the post-process fallback is mandatory. Document the negative result in the PR.
- **Edge case** — `gh release delete-asset` (fallback path) must run after cargo-dist uploads but before `announce` finalizes, so the release is never publicly observed in the bloated state. Explicit ordering verification on rc.7 is required.
- **Risk** — dropping `dist-manifest.json` breaks any cargo-dist-style shell/PowerShell installer that reads it to resolve platform assets. The project does not currently advertise such an installer path (only the .exe/.dmg/.deb/.rpm), so this is acceptable today. Re-evaluate if a manifest-driven installer is reintroduced later.
- **Missing input** — confirm no external tooling, README link, or documentation currently URL-links to the per-arch archives. A repo-wide grep + a quick web search of the project name should suffice; surface anything found.

## Original Description

Trim release-pipeline assets to just the installer-style artifacts plus provenance, dropping the 20 cargo-dist intermediate per-arch archives (app-*.tar.xz, ad-window-*.tar.xz, and their .sha256 sidecars).

Currently the GitHub release contains those 20 intermediates because cargo-dist's default workspace config publishes them as release assets — but they're only meaningful as inputs to the custom deb/rpm/dmg/nsis package jobs, which already consume them via actions/download-artifact within the pipeline. They add noise for end users.

Desired end state: the release attaches only:
- Installer-style: yt-dlp-ui-installer.exe, yt-dlp-ui-universal.dmg, the two .deb files, the two .rpm files
- Provenance: sha256.sum, dist-manifest.json, source.tar.gz + .sha256

Approach should keep the per-arch archives as **workflow** artifacts (so the package jobs still download them) but exclude them from the published release. Two viable mechanisms:
- Configure cargo-dist (e.g. dist-workspace.toml) to mark the archives as build-only / non-publishable, or
- Post-process in the release.yml `host` step to delete the unwanted assets via `gh release delete-asset` before the `announce` step finalises.

Pitfalls / things to verify:
- The deb/rpm/dmg/nsis package jobs MUST continue to receive the per-arch archives (they call actions/download-artifact with the artifacts-build-local-* names; these are workflow artifacts, not release assets, so dropping them from the release shouldn't break the chain — but verify after the change).
- Do not touch source.tar.gz, sha256.sum, or dist-manifest.json — those stay.
- After change, run a release tag end-to-end (e.g. v0.5.1-rc.7) to confirm: installers still build, all package jobs still pass, and the final release page lists only the trimmed asset set.
- Pre-existing v0.5.0 / v0.5.1-rc.* releases keep their bloated asset list; this only affects future tags.

Do not implement now — just capture the UC. The user will tackle it later in isolation.

## Clarifications

- Q: Which implementation mechanism do you prefer for trimming release assets?
  A: Try cargo-dist config first; fall back to `gh release delete-asset` post-process if no clean switch exists.
- Q: Keep `dist-manifest.json` on the trimmed release?
  A: Drop it too. (Note: this overrides the "Desired end state" listed in the original description, which had `dist-manifest.json` in the keep list.)
- Q: Apply the trim retroactively to existing releases?
  A: No — future tags only.
