# Use Case 24: Fix Windows Release build (modern gpg via winget)

## Summary
Windows builds in cargo-dist's Release pipeline fail at `scripts/fetch-yt-dlp.ps1`'s `gpg --import` step. Git for Windows ships gpg 1.4.x at `C:\Program Files\Git\usr\bin\gpg.exe`; upstream yt-dlp's ASCII-armored signing key requires gpg 2.x to import. UC 20's developer dropped the failing `choco install gnupg` step but left the Git-for-Windows fallback in place, which doesn't actually work for the import path. This UC installs modern gpg 2.x via `winget install --id GnuPG.GnuPG --silent --accept-source-agreements --accept-package-agreements` in `ci/dist-build-setup.yml`'s Windows path AND `package-nsis.yml`. **Local-first verification:** the user has a Windows machine and tackles this UC there directly — no `act`, no VM, no GHA carve-out. The user runs `winget install GnuPG.GnuPG`, then `gpg --version` to confirm 2.x, then exercises `pwsh scripts/fetch-yt-dlp.ps1` against a known-good fixture before pushing.

## Acceptance Criteria
1. Windows x86_64 build-local-artifacts succeeds end-to-end on the GHA Release pipeline (build-local → custom-package-nsis → installer `.exe` artifact emitted).
2. `ci/dist-build-setup.yml` adds `winget install --id GnuPG.GnuPG --silent --accept-source-agreements --accept-package-agreements` on Windows runners. Idempotent guard (e.g. `winget list --id GnuPG.GnuPG --exact > $null; if ($LASTEXITCODE -ne 0) { winget install ... }`) to avoid re-install on cache-warm runners.
3. `package-nsis.yml` mirrors the same winget install step (each package job runs on a fresh runner; same shape as the existing yt-dlp / deno fetches).
4. The existing `Smoke-check gpg availability` step in `package-nsis.yml` is updated to assert `gpg --version` reports `2.x` (not just exit 0). Old fallback to Git-for-Windows's gpg 1.4.x is removed entirely (no env-detection branching).
5. `scripts/fetch-yt-dlp.ps1` does NOT need code changes — once gpg 2.x is on PATH, the existing `& gpg --import` invocation works.
6. **Local verification gate (load-bearing on the user's Windows machine):**
   - `winget install --id GnuPG.GnuPG --silent --accept-source-agreements --accept-package-agreements`
   - `gpg --version` reports 2.x.
   - Run `pwsh scripts/fetch-yt-dlp.ps1 x86_64-pc-windows-msvc C:\temp\yt-dlp-test\` against a real release tag (e.g. `$env:YT_DLP_VERSION = '2026.03.17'; $env:REPO_ROOT = '<path-to-clone>'`). Must produce a verified `yt-dlp.exe` binary in the output dir.
   - Captured in the commit message: winget install output, `gpg --version`, fetch script's success line.
7. The dev team does NOT push the commit until AC #6 passes on the user's Windows machine.
8. ci.yml stays green throughout.
9. **GHA budget:** at most ONE GHA Release run is consumed for this UC's validation. If winget itself flakes on `windows-latest` (different from the user's local Windows), the commit message MUST include rollback instructions to either `actions/setup-gnupg` or Git-for-Windows mingw64 detection (`C:\Program Files\Git\mingw64\bin\gpg.exe` is gpg 2.x on modern Git for Windows installs).
10. After this UC merges and GHA confirms green, the team-lead force-moves the v0.5.0 tag to the post-UC-24 HEAD and triggers ONE final end-to-end Release run. All four installer artifacts (.deb, .rpm, .dmg, .exe) emit. UC 20 is then fully closed.

## Potential Pitfalls & Open Questions
- **Edge case** — `winget` requires App Installer 1.16+ on Windows 10; `windows-latest` GHA runners ship Windows Server 2022 with App Installer current. If a future runner image regresses, winget might fail; the commit's rollback instructions cover this.
- **Edge case** — `winget install GnuPG.GnuPG` adds gpg to PATH at `C:\Program Files (x86)\gnupg\bin\` typically. PATH ordering matters: if Git for Windows's gpg 1.4.x is earlier on PATH, the wrong gpg is invoked. The smoke check in AC #4 catches this; if it triggers, prepend the new gpg dir to PATH explicitly in `dist-build-setup.yml`.
- **Edge case** — User's local Windows machine might already have a gpg installed (via a prior winget install, scoop, or chocolatey). The local verification's `gpg --version` could pass via THAT pre-existing gpg, masking a problem on a clean GHA runner. The PR description should note "local verify ran on a clean PATH" or document the user's local gpg provenance.
- **Assumption** — UC 21 (deno parser) and ideally UC 22 (Linux deps) are merged first. Windows is the last OS UC because it's the only one with the GHA-only fallback risk if winget regresses.
- **Risk** — winget is a higher-velocity ecosystem than GHA actions; package metadata changes could land between local verify and GHA validation. Single-shot validation budget acknowledged.

## Original Description
Part of UC 21–24 (split per-OS at the user's request, since they have a Windows machine and can verify locally). This is the Windows-specific Release-build fix. Local-first via the user's actual Windows machine (no `act` / VM / carve-out needed). Lands LAST among the four UCs; once merged + GHA-green, the v0.5.0 tag is force-moved to the post-UC-24 HEAD for a final end-to-end Release run.
