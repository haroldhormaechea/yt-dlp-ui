# 0010 — ffmpeg bundling (LGPL-only, per-OS source posture)

- **Status:** accepted
- **Date:** 2026-05-07

## Context

YouTube serves modern qualities (1080p+) via DASH, splitting the audio and
video into separate streams that the client must merge. yt-dlp delegates
this merge to a working `ffmpeg` invocation. UC 06 closed the
yt-dlp-bundling gap; until UC 17, ffmpeg was still expected on the user's
PATH. For non-technical users that meant either silently degraded output
(separate `.m4a` + `.mp4`) or hard failures.

We need to bundle a working ffmpeg per OS. Three constraints shape the
choice:

1. **License posture.** PolyForm Noncommercial + LGPL-only deny-list
   (`deny.toml`) means we cannot ship a GPL ffmpeg build. No x264, x265,
   fdk-aac, libxvid, libvpx-with-vp8 GPL-tainted features — LGPL-only.
2. **Per-OS prebuilt availability.**
   - **Linux + Windows:** BtbN/FFmpeg-Builds publishes well-curated LGPL
     static archives, both GA-stable (`n7.x`) and rolling (`master`).
   - **macOS:** no LGPL-only mainstream prebuilt exists. evermeet.cx is
     x86_64-only (we need universal-binary parity per UC 06). Homebrew
     and MacPorts pull in GPL transitively. The cleanest path is to
     build from upstream FFmpeg source on the GHA macOS runner.
3. **Supply-chain story.** Match the yt-dlp posture (UC 06): pinned
   versions, SHA256 verification, manual-PR bumps.

## Decision

**Per-OS source posture:**

| Target | Source | Verification |
|---|---|---|
| Linux x86_64 / arm64 | `BtbN/FFmpeg-Builds` `*-linux64-lgpl-*.tar.xz` and `*-linuxarm64-lgpl-*.tar.xz` (release-tag, NOT master-latest) | In-tree SHA256 pin in `scripts/runtime-deps-pins.env` + remote `<asset>.sha256` defense-in-depth |
| Windows x86_64 | `BtbN/FFmpeg-Builds` `*-win64-lgpl-*.zip` | Same as Linux |
| macOS arm64 + x86_64 | Built from upstream FFmpeg source on GHA macOS runners (both arches), lipo-merged | Source-tarball SHA256 pin in `scripts/runtime-deps-pins.env` + configure-line lint via `ffmpeg -version` |

The macOS configure invocation is locked to:

```
--disable-gpl --disable-nonfree
--disable-libx264 --disable-libx265 --disable-libfdk-aac
--disable-libxvid --disable-libvpx --disable-libmp3lame
--enable-securetransport --enable-zlib
--enable-static --disable-shared
--disable-doc --disable-htmlpages --disable-manpages
--disable-podpages --disable-txtpages
--disable-debug --disable-ffplay
```

`--enable-libopus` and `--enable-libvorbis` were dropped from the locked
set after the pre-PR M-series Mac smoke surfaced a `pkg-config` lookup
failure: neither library is on a stock macOS install and forcing every
dev host (and the GHA `macos-latest` runner) to `brew install opus
libvorbis` widens the host-environment footprint without buying us
anything UC 17 actually needs. yt-dlp's DASH-merge path remuxes the
existing opus / vorbis bytestream into the output container without
re-encoding, so the built-in demuxers suffice. If a future UC needs
opus / vorbis re-encoding, add the `brew install` step in
`package-dmg.yml` and the corresponding `--enable-lib*` flags in
lock-step.

with `MACOSX_DEPLOYMENT_TARGET=11.0` so the binary runs on macOS Big Sur
and later. The build script's tail re-invokes the freshly built ffmpeg
with `-version` and grep-checks for any of `--enable-libx264|libx265|
libfdk-aac|gpl|nonfree`; a hit fails the build with `exit 75` so a
configure-flag regression cannot ship.

**Path resolver.** `crates/app/src/paths.rs` exposes
`bundled_ffmpeg_path() -> Result<PathBuf, PathError>` mirroring
`bundled_yt_dlp_path()`. The `Result` (rather than `Option`) is
intentional: it forces every caller to decide whether `BundledMissing`
is fatal or recoverable. The single call site in `lib.rs::run`
downgrades to `Option<PathBuf>` after logging, so the rest of the app
carries an `Option`.

**Spawn-time gate.** `DownloadManager::spawn_download_for` refuses to
call `bridge.start_download` when `ffmpeg_path.is_none()`. Audio-only
formats are NOT exempted — yt-dlp's ExtractAudio postprocessor needs
ffmpeg too, and an exemption based on the requested format would be
fragile (a future format-selection tweak could silently break it).

**Build-script failure-mode posture.** `crates/app/build.rs::bundle_dev_ffmpeg`
warns-not-fails on any error: missing `runtime-deps/ffmpeg`, fetch
script failure, copy failure, all surface as `cargo:warning=` and the
build continues. The runtime spawn-time gate is the single clear
failure point; making `cargo build` itself fail just because a developer
on a flight has no network is a worse UX.

**Quarantine-strip helper.** `paths::strip_macos_quarantine_if_needed()`
runs `xattr -d com.apple.quarantine` on each bundled binary at startup
on macOS. The package-dmg job ALSO runs `xattr -cr` on the staged `.app`
before `hdiutil create`, which is the bigger lever — but the runtime
helper is defense-in-depth in case a future signing posture or an end-
user-modified install introduces fresh xattrs.

## Consequences

**Positive:**
- Non-technical users on macOS / Windows / Linux get DASH-merge "just
  works" with no PATH dependency.
- LGPL-only posture aligns with `cargo-deny`'s denial of GPL/AGPL/SSPL.
- Per-OS supply chain mirrors yt-dlp's: pinned, SHA-verified, manual
  bumps.
- Type-discipline (`Result` not `Option`) at the resolver forces every
  caller to handle missing-ffmpeg explicitly. The spawn-time gate is the
  single user-visible failure surface.

**Negative:**
- **Bundle size.** LGPL-static ffmpeg adds ~25–35 MB per OS to each
  installer. The PROJECT_BRIEF.md § Performance budgets ceiling moves
  from 50 MB → 100 MB to absorb this; if a later compression sweep brings
  the measured size back under 50 MB, the budget reverts.
- **macOS build time.** ~10–15 minutes per arch on the GHA `macos-latest`
  runner. The `package-dmg` workflow keys an `actions/cache@v4` entry on
  the source-tarball SHA + the build-script hash so cache hits skip the
  build entirely. Cold builds add wall-clock to the release pipeline; on
  cache hit the cost is the lipo-merge only.
- **More CI surface to maintain.** Two extra fetch scripts
  (`fetch-ffmpeg.sh` / `.ps1`), one source-build script
  (`build-ffmpeg-macos.sh`), and four pin fields in
  `runtime-deps-pins.env`. Bump procedure documented in
  `scripts/README.md` § Bump procedure.
- **No GPG signature verification.** BtbN/FFmpeg-Builds does not publish
  detached GPG signatures; we make do with in-tree SHA pin + remote
  per-asset `.sha256`. macOS source-build uses an in-tree pin against
  upstream's published tarball — the upstream FFmpeg project publishes
  PGP signatures for source tarballs but the verification keyring would
  be one more long-lived rotation problem; SHA-only-with-in-tree-pin is
  the same posture as deno (T11) and is documented in THREATS.md as an
  acknowledged weaker link.

## Alternatives considered

- **SHA-only verification, no in-tree pin.** Rejected — a compromise of
  the BtbN release would substitute both the binary and its `.sha256`
  in lock-step. The in-tree pin breaks that lock-step and forces a
  rotation through human PR review.
- **Resolver returns `Option` instead of `Result`.** Rejected — every
  intermediate caller would silently downgrade to `None` and the
  spawn-time gate's error message would be the only signal. Type-
  forcing the explicit handling at the resolver makes the
  release-vs-dev branching legible at the single `lib.rs::run` site.
- **Narrow ffmpeg-path plumbing into download.rs only.** Rejected by
  the proposal — yt-dlp's metadata calls (`expand_playlist`,
  `get_title*`, `get_thumbnail_url`) also occasionally invoke ffmpeg
  for thumbnail decoding edge cases. Plumbing the path into all four
  is mechanical and avoids future bug-hunting on a metadata path that
  silently degrades.
- **Rosetta x86_64 ffmpeg on Apple Silicon at runtime.** Rejected —
  forces every spawn through Rosetta translation, costs ~20% ffmpeg
  CPU, and does not fix the universal-binary parity requirement from
  UC 06.
- **evermeet.cx for x86_64 only on macOS.** Rejected — UC 06 mandates
  a universal-binary `.dmg`; shipping x86_64-only ffmpeg would mean
  Apple Silicon users hit Rosetta on every download.
- **`master-latest` BtbN tag.** Rejected by the use case directly —
  pinned release tag only, so a SHA-pin rotation is a deliberate human
  action.

## References

- PROJECT_BRIEF.md § Architecture § Bundled-binary path
- PROJECT_BRIEF.md § Performance budgets (bundle-size)
- PROJECT_BRIEF.md § Deployment
- THREATS.md § T13 (this UC)
- THREATS.md § T7 (license drift extension to bundled native binaries)
- use-cases/17-merge-audio-and-video-with-ffmpeg.md
- use-cases/06-bundle-binaries-in-installers.md (parallel posture)
- ADR 0005 (yt-dlp bundling — same supply-chain posture template)

## Slot-numbering note

The use-case file proposed slot 0007. That slot was already taken by
ADR `0007-design-system.md`. ADRs are append-only with a monotonically
increasing slot number; reusing 0007 would produce a confusing diff
history. This ADR claims slot 0010 — the next free integer at the time
of writing — and the use-case file's reference is updated accordingly.
