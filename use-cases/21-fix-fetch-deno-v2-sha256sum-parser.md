# Use Case 21: Fix fetch-deno scripts for deno v2.x `.sha256sum` format

## Summary
Deno v2.x's `.sha256sum` file format differs from v1.x. UC 17's `scripts/fetch-deno.sh` and `scripts/fetch-deno.ps1` parsers were written for v1.x. After UC 20 bumped `DENO_VERSION` to `v2.7.14`, the parsers fail with "could not parse <file>" on every platform's deno fetch. This UC updates both scripts to parse the v2.x format and adds a bats fixture + pwsh test pinning the new shape. The fix is **OS-agnostic** — pure script logic — and is the first of four UCs (UC 21–24) that together resolve the cross-platform Release-build failures left over from UC 20. **Local-first verification:** `bats scripts/test-fetch-deno.bats` (new file) + the existing pwsh argv-contract test, both runnable on the user's M-series Mac without Docker. No GHA Release run is consumed by this UC; ci.yml's existing bats + pwsh jobs catch regressions on push.

## Acceptance Criteria
1. `scripts/fetch-deno.sh` parses deno v2.x's `.sha256sum` file format and extracts the SHA correctly for every supported target triple (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`).
2. `scripts/fetch-deno.ps1` parses the same v2.x format identically.
3. New `scripts/test-fetch-deno.bats` file: hermetic bats test using **two** committed fixtures (Linux and Windows), since deno v2.x emits **two different `.sha256sum` shapes** depending on the build runner — GNU coreutils format for Unix triples, PowerShell `Get-FileHash | Format-List` output for `x86_64-pc-windows-msvc`. Capture both via `curl -fsSL "https://github.com/denoland/deno/releases/download/v2.7.14/deno-<triple>.zip.sha256sum"` into `scripts/tests/fixtures/deno-v2.7.14-unix.sha256sum` (using `x86_64-unknown-linux-gnu`) and `scripts/tests/fixtures/deno-v2.7.14-windows.sha256sum` (using `x86_64-pc-windows-msvc`). Test cases:
   - Positive: parser extracts the correct 64-char SHA from the **Unix-format** fixture.
   - Positive: parser extracts the correct 64-char SHA from the **Windows-format** fixture.
   - Negative: malformed format → exit non-zero with a clear error message.
   - Negative: empty file → exit non-zero.
   - Negative: SHA mismatch in the fixture → exit 73 (existing convention).
4. `scripts/test-fetch-ps1.ps1` extended with deno-v2 parsing cases mirroring the bats fixtures.
5. **Local verification gate:** the dev team runs `bats scripts/test-fetch-deno.bats` AND `pwsh scripts/test-fetch-ps1.ps1` (or its existing-on-macOS equivalent) locally and captures the passing output in the commit message before pushing.
6. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` stay green.
7. No regression to UC 17's `scripts/fetch-yt-dlp.{sh,ps1}` and `scripts/fetch-ffmpeg.{sh,ps1}` (they have separate sha-verification paths; this UC only touches deno).
8. The PR for this UC is reviewed and merged BEFORE UC 22 / UC 23 / UC 24 are tackled. Every other Release-build UC depends on the deno fetch step succeeding.
9. No new third-party Rust crates. No new bash / pwsh dependencies.

## Potential Pitfalls & Open Questions
- **Edge case** — Locking out v1.x re-pinning: once the parser is v2-only, bumping `DENO_VERSION` back to a v1.x value would break. Document the v2-only assumption in `scripts/README.md` § "Bump procedure".
- **Edge case** — deno's release process might tweak the format again between v2.x patch releases. The fixture should match the EXACT file format upstream emits today; if a future v2.y release changes shape, this UC's parser may need a follow-up.
- **Risk** — bats fixtures are committed to the repo (small text files); the SHA pin in `runtime-deps-pins.env` continues to be the canonical truth. Fixture is for parser unit-testing only, not for integration verification.

## Original Description
Part of UC 21–24 (split per-OS at the user's request, since they have all three OSes available locally). This UC is the OS-agnostic deno-parser fix — first to land because all platforms hit it. The fix is local-first by nature: bats + pwsh tests catch the regression without a GHA Release run.
