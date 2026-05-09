# Use Case 22: Fix Linux Release build (gdk-3.0 + WebKit deps in dist-build hook)

## Summary
Linux builds in cargo-dist's Release pipeline fail with `gdk-sys v0.18.2 build fail: gdk-3.0 not found`. The `gdk-sys` crate is pulled by `rfd` (file-dialog dependency in `crates/app`). UC 17's `ci/dist-build-setup.yml` prebuild hook fetches binaries but never installs Linux native deps the way `ci.yml`'s test job does. This UC adds the missing `apt-get install` step gated by `runner.os == 'Linux'`. Affects both `aarch64-unknown-linux-gnu` and `x86_64-unknown-linux-gnu` build-local-artifacts targets. **Local-first verification:** `docker run --rm --platform linux/amd64` (and `--platform linux/arm64` via Docker Desktop's pre-installed QEMU buildx) reproducing the failing `cargo build --profile=dist -p app` flow with the new deps installed. Tackled on the user's M-series Mac with Docker Desktop. No GHA Release run is consumed until the docker pre-flight passes both arches.

## Acceptance Criteria
1. Linux x86_64 build-local-artifacts succeeds end-to-end on the GHA Release pipeline.
2. Linux arm64 build-local-artifacts succeeds end-to-end.
3. `ci/dist-build-setup.yml` installs the following packages on Linux runners (gated by `runner.os == 'Linux'`): `libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libssl-dev pkg-config build-essential`. **Note:** `package-deb-rpm.yml` does NOT install GTK/WebKit/SSL apt deps today (it only does `sudo mv nfpm` — nfpm is a Go binary that consumes prebuilt artifacts; no Rust compile in that job). The earlier draft of this AC referenced "duplicate installs in `package-deb-rpm.yml`" — that was a misremember of UC 20's `ci.yml` install; corrected here. No consolidation work in `package-deb-rpm.yml` is needed.

   **Mandatory same-commit co-modification:** because cargo-dist's splice mechanism is build-time (not runtime), `.github/workflows/release.yml` MUST be regenerated (or hand-edited) in the same commit so the inlined splice picks up the new step. Without that, GHA runs the old `release.yml` and the install is invisible at workflow-run time. Preferred path: `dist generate --mode ci` from the repo root. Fallback: hand-edit `release.yml` between the existing `Install Rust non-interactively if not already installed` step and the `Fetch yt-dlp + deno (Linux/macOS)` step, matching the YAML quoting style of surrounding inlined-splice steps.
4. **Local verification gate (load-bearing):** before pushing, the dev team runs both arches:
   ```
   docker run --rm --platform linux/amd64 -v $(pwd):/work -w /work ubuntu:24.04 bash -c \
     "apt-get update && \
      apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libssl-dev pkg-config build-essential curl gpg ca-certificates && \
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal && \
      . ~/.cargo/env && cargo build --profile=dist -p app"
   ```
   Then repeat with `--platform linux/arm64` (Docker Desktop pre-installs QEMU). Both must complete with no `gdk-sys` error. The arm64 run takes ~30 min under emulation; acceptable since it runs once. Captured in the commit message.
5. The dev team does NOT push the commit until AC #4 passes both platforms.
6. ci.yml stays green throughout.
7. No new third-party Rust crates.
8. No regression to UC 20's structural correctness or UC 17's LGPL-only ffmpeg posture.
9. **GHA budget:** at most ONE GHA Release run is consumed for this UC's validation. If the docker pre-flight passed locally but the GHA run fails, escalate immediately rather than iterating push-and-watch.

## Potential Pitfalls & Open Questions
- **Edge case** — Ubuntu version drift: `ubuntu:24.04` (Noble) is what `ubuntu-latest` GHA runners ship today. If GHA bumps the runner to a future LTS that Docker Desktop's Noble image doesn't track, the local pre-flight might pass while GHA fails on package-name rename. Pin the docker image version explicitly.
- **Edge case** — `webkit2gtk-4.1` vs. `webkit2gtk-4.0`: the dependency is `4.1-dev` per `wry`'s docs at the pinned `wry` version. If `wry` bumps to a new major, the apt package name might change.
- **Edge case** — `ci.yml`'s test job already installs these deps (UC 20 added them); this UC adds the same set to `dist-build-setup.yml`. The two locations stay in sync manually.
- **Assumption** — UC 21 (deno-parser) is merged before this UC starts. Otherwise the docker pre-flight would also hit the deno fetch failure mid-build.
- **Risk** — `cargo build --profile=dist` differs slightly from cargo-dist's internal invocation; the local docker pre-flight is "necessary but not sufficient." Single GHA run after local-pass is the real gate.

## Original Description
Part of UC 21–24 (split per-OS at the user's request). This is the Linux-specific Release-build fix. Tackled on the user's M-series Mac with Docker Desktop, exercising both `linux/amd64` and `linux/arm64` via QEMU-backed buildx. Depends on UC 21 (deno parser) being merged first.
