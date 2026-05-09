# Use Case 23: Fix macOS x86_64 Release build (nasm for ffmpeg cross-build)

## Summary
macOS x86_64 builds in cargo-dist's Release pipeline fail at `scripts/build-ffmpeg-macos.sh`'s `configure` step with `nasm/yasm not found or too old. Use --disable-x86asm for a crippled build`. The `arch -x86_64` Rosetta cross-build needs nasm for x86 SIMD assembly; the native arm64 build in UC 17 doesn't, which is why aarch64-apple-darwin is the only Release target that worked in UC 20's first attempt. This UC adds `brew install nasm` to `ci/dist-build-setup.yml`'s macOS path. **Local-first verification:** `arch -x86_64 bash scripts/build-ffmpeg-macos.sh /tmp/ffmpeg-x86-test/` runs natively on the user's M-series Mac via Rosetta — no Docker / no VM / no emulation. Single command pre-flight; no GHA run consumed during local validation.

## Acceptance Criteria
1. macOS x86_64 build-local-artifacts succeeds end-to-end on the GHA Release pipeline.
2. macOS arm64 build-local-artifacts continues to succeed (regression guard — UC 20's only previously-working target).
3. The universal `.dmg` lipo-merge in `installer/build-macos-dmg.sh` produces a fat Mach-O binary with both architectures (`lipo -info` reports both `x86_64` and `arm64`).
4. `ci/dist-build-setup.yml` adds `brew install nasm` (or `brew install nasm yasm` if either suffices for ffmpeg's configure) on macOS runners. Add idempotently (`brew list nasm &>/dev/null || brew install nasm`) since brew install is slow on cache-warm runners.
5. **Local verification gate (load-bearing):** before pushing, the dev team runs:
   ```
   arch -x86_64 bash scripts/build-ffmpeg-macos.sh /tmp/ffmpeg-x86-test/
   lipo -info /tmp/ffmpeg-x86-test/ffmpeg
   /tmp/ffmpeg-x86-test/ffmpeg -version | head -1
   /tmp/ffmpeg-x86-test/ffmpeg -version | grep -E -- '--enable-libx264|--enable-libx265|--enable-libfdk-aac|--enable-gpl|--enable-nonfree' && echo "FORBIDDEN FLAG PRESENT" && exit 1 || echo "LGPL-only confirmed"
   ```
   First command must succeed (build the x86_64 ffmpeg from source under Rosetta). `lipo -info` must report `x86_64`. `ffmpeg -version` must run cleanly. Forbidden-flag check must report "LGPL-only confirmed" (UC 17's four-layer guard). Captured in the commit message.
6. The dev team does NOT push the commit until AC #5 passes (~5–10 min build time on M-series Rosetta).
7. ci.yml stays green throughout.
8. No regression to UC 17's LGPL-only ffmpeg posture (no GPL components, no x264/x265/libfdk-aac, no `--enable-gpl`).
9. **GHA budget:** at most ONE GHA Release run is consumed for this UC's validation.

## Potential Pitfalls & Open Questions
- **Edge case** — Brew might be slow on cache-cold runners. Idempotent guard (`brew list nasm &>/dev/null || brew install nasm`) avoids re-installs but doesn't cache. Acceptable for now; an upstream `actions/cache` for `~/.cache/Homebrew/downloads` could shave time later.
- **Edge case** — Local Rosetta build emits warnings about `MACOSX_DEPLOYMENT_TARGET` differences if user's macOS deploy target differs from the script's pinned `11.0`. UC 17 already pins this; verify the local build emits an x86_64 binary that runs on macOS 11+.
- **Edge case** — User's local nasm version (whatever `brew install nasm` provided in their environment, possibly older) vs. GHA runners. If the GHA run fails after local passes, the version drift might be the cause; pin nasm version in `dist-build-setup.yml` if needed.
- **Assumption** — UC 21 (deno parser) is merged first; otherwise the build would fail earlier on deno fetch.
- **Risk** — `arch -x86_64` Rosetta on the user's M-series isn't bit-identical to a native x86_64 macOS runner. Local pass is necessary but not sufficient; the single GHA run after is the real gate.

## Original Description
Part of UC 21–24 (split per-OS at the user's request). This is the macOS-specific Release-build fix. Tackled on the user's M-series Mac via `arch -x86_64`. Depends on UC 21 (deno parser) being merged first.
