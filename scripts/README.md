# Release-time fetch scripts

This directory holds the per-binary release-time hooks that the GitHub Actions
release workflow invokes before `cargo-dist` packages each artifact. They
fetch the upstream `yt-dlp` and `deno` binaries, verify them, and place them
in `runtime-deps/` for cargo-dist's `include` mechanism (see
`dist-workspace.toml` § `include`).

| Script                | Shell      | Used by runners       | Verification        |
|-----------------------|------------|-----------------------|---------------------|
| `fetch-yt-dlp.sh`     | bash       | `ubuntu-*`, `macos-*` | SHA256 + GPG        |
| `fetch-yt-dlp.ps1`    | PowerShell | `windows-*`           | SHA256 + GPG        |
| `fetch-deno.sh`       | bash       | `ubuntu-*`, `macos-*` | SHA256              |
| `fetch-deno.ps1`      | PowerShell | `windows-*`           | SHA256              |
| `fetch-ffmpeg.sh`     | bash       | `ubuntu-*`            | SHA256 (in-tree pin + remote per-asset) |
| `fetch-ffmpeg.ps1`    | PowerShell | `windows-*`           | SHA256 (in-tree pin + remote per-asset) |
| `build-ffmpeg-macos.sh` | bash     | `macos-*`             | SHA256 of source tarball + configure-flag lint |

The yt-dlp scripts verify against `scripts/keys/yt-dlp.asc` (upstream yt-dlp's
GPG public key, controlled by upstream). Deno does not publish GPG signatures;
SHA-only is documented in `THREATS.md` § T11.

## Argv contract (all four scripts)

```
<script> <target-triple> <output-dir>
```

`<target-triple>` is a Rust target triple (`aarch64-apple-darwin`,
`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`x86_64-apple-darwin`, `x86_64-pc-windows-msvc`).

`<output-dir>` is created if absent and receives the canonical-name
binary — `yt-dlp` or `deno` — **with no extension on any OS** (see
"Canonical name" below).

## Required env

| Script             | Env var          | Example       |
|--------------------|------------------|---------------|
| `fetch-yt-dlp.*`   | `YT_DLP_VERSION` | `2026.04.21`  |
| `fetch-deno.*`     | `DENO_VERSION`   | `1.47.2`      |
| `fetch-ffmpeg.*` / `build-ffmpeg-macos.sh` | (none — sourced from `runtime-deps-pins.env`) | — |

`YT_DLP_VERSION` and `DENO_VERSION` are pinned in `ci/dist-build-setup.yml`
(NOT in `release.yml`, which `dist generate --mode ci` regenerates and would
strip top-level `env:` blocks). The ffmpeg pins live in
`scripts/runtime-deps-pins.env` (sourced by Bash, parsed by PowerShell);
the env file is the single source of truth for both the BtbN release tag,
the in-tag stable-release tag, the per-arch SHA256 digests, and the
upstream FFmpeg source tarball pin used by the macOS source build.

## Bump procedure

### yt-dlp / deno

1. Edit `ci/dist-build-setup.yml`, change the pinned `YT_DLP_VERSION` or
   `DENO_VERSION`.
2. Verify the upstream release exists and that asset names still match the
   case-statement in the fetch scripts:
   ```sh
   gh release view "${YT_DLP_VERSION}" --repo yt-dlp/yt-dlp \
       --json assets --jq '.assets[].name'
   ```
3. Open a PR. Re-running `dist generate --mode ci` is NOT required for an
   env-var bump (it doesn't touch the workflow shape).
4. Tag a release once merged.

### ffmpeg

1. Pick a new BtbN release tag (`autobuild-YYYY-MM-DD-HH-MM`). Avoid
   `master-latest` — only release-tagged autobuilds are pinnable.
2. Pick the matching in-tag stable-release tag (`n7.x` or `n8.x`).
   The asset filename pattern is `ffmpeg-${FFMPEG_RELEASE_TAG}-<arch>-lgpl-<minor>.{tar.xz,zip}`.
3. Compute fresh SHA256 digests:
   ```sh
   for asset in linux64 linuxarm64 win64; do
       arch_ext="$( [[ ${asset} == win64 ]] && echo zip || echo tar.xz )"
       url="https://github.com/BtbN/FFmpeg-Builds/releases/download/${FFMPEG_VERSION}/ffmpeg-${FFMPEG_RELEASE_TAG}-${asset}-lgpl-7.1.${arch_ext}"
       printf '%s  %s\n' "$(curl -fsSL "${url}" | shasum -a 256 | awk '{print $1}')" "${asset}"
   done
   ```
4. (Optional) Bump the macOS source pin: pick a fresh `FFMPEG_VERSION_SOURCE`
   from <https://ffmpeg.org/releases/> and compute its tarball SHA256.
5. Edit `scripts/runtime-deps-pins.env`, paste the new values.
6. Smoke-test locally with `just fetch-runtime-deps` and verify the
   resulting binary banner does NOT contain
   `--enable-libx264|libx265|libfdk-aac|gpl|nonfree`.
7. Open a PR. Re-running `dist generate --mode ci` is NOT required for a
   pin bump (it doesn't touch the workflow shape).

### SHA-only-with-in-tree-pin posture

ffmpeg has no upstream GPG-signed prebuilts (BtbN does not publish
detached signatures, and we did not chain trust through upstream FFmpeg's
own PGP key for the source-build path). Verification posture is
defense-in-depth SHA-only:

1. **In-tree SHA pin** in `runtime-deps-pins.env` — primary check. A
   compromise of the BtbN release would not match the pin we landed in
   a reviewed PR.
2. **Remote per-asset `<asset>.sha256`** — secondary check, downloaded
   alongside the binary. Catches a state where the in-tree pin was
   compromised without rotating BtbN's published checksum.
3. **macOS configure-flag lint** in `build-ffmpeg-macos.sh` — terminal
   sanity check on the built binary's banner. Refuses to ship a binary
   whose configuration contains GPL or nonfree flags.

This is the same posture as deno (THREATS.md § T11). Re-evaluate if
upstream FFmpeg or BtbN ships Sigstore / cosign release attestations.

## Dual-script discipline

The `.sh` and `.ps1` parallels MUST stay in sync. Every fix to one shell
implementation lands in the other in the same PR. Specifically:

- Asset-name → target-triple map.
- Exit codes (64 usage, 65 missing env, 72 asset not in sums, 73 SHA
  mismatch, 74 GPG fail, 75 yt-dlp.asc missing, 70 no sha tool, 71 missing
  archive entry).
- Canonical destination filename (`yt-dlp` and `deno`, no extension, on
  every OS — see below).

CI runs both `scripts/test-fetch-yt-dlp.bats` and `scripts/test-fetch-ps1.ps1`
on every push and pull request via `.github/workflows/ci.yml`.

## Canonical name (no `.exe` on Windows)

cargo-dist 0.31.0's `include` directive is a single global list with no
per-target pruning, and it fails the build when a listed file is missing
(verified via Smoke 1 of UC 06). To keep `include` to two entries —
`runtime-deps/yt-dlp` and `runtime-deps/deno` — both fetch scripts produce
single-name binaries on every OS, including Windows.

The Windows branch of `crates/app/src/paths.rs::expected_bundled_path_from`
probes `<bin>.exe` first (so an admin who manually renames the binary still
works), then `<bin>.cmd` (UC 03 dev wrapper, debug builds only), then the
canonical `<bin>` (no extension; the path actually used in installers).

## Release-readiness checklist

Each `package-*.yml` referenced workflow re-fetches via these scripts (the
`runtime-deps/` from `build-local-artifacts` does NOT survive into the
package job — different runner, fresh checkout). A failed `package-dmg.yml`
does not block `.deb` upload; the three packagers are orthogonal jobs in the
`global-artifacts-jobs` splice.

Before tagging a release, locally smoke-test:

```sh
# yt-dlp + deno fetch on the host's native target.
YT_DLP_VERSION=<pin> bash scripts/fetch-yt-dlp.sh "$(rustc -vV | sed -n 's/^host: //p')" runtime-deps/
DENO_VERSION=<pin>   bash scripts/fetch-deno.sh   "$(rustc -vV | sed -n 's/^host: //p')" runtime-deps/

# Local cargo-dist build for that target.
dist build --artifacts=local --target="$(rustc -vV | sed -n 's/^host: //p')"
```

`tar -tvf target/distrib/app-*.tar.xz | grep -E '(yt-dlp|deno)$'` should show
`-rwxr-xr-x` permissions on both bundled binaries.
