# yt-dlp-ui — common development commands.
#
# `just` is cross-platform (Linux / macOS / Windows). Install via:
#   - macOS:    brew install just
#   - Linux:    cargo install just  (or your distro's package manager)
#   - Windows:  winget install --id Casey.Just  (or scoop install just)
#
# Run `just` with no argument for the default recipe.

default: lint test

run:
    cargo run --bin app

adwin:
    cargo run --bin ad-window

fake-ad-server:
    cargo run --example fake-ad-server

test:
    cargo test --workspace

lint:
    cargo clippy --workspace --all-targets -- -D warnings

fmt:
    cargo fmt --all

audit:
    cargo audit

deny:
    cargo deny check

coverage:
    cargo llvm-cov --workspace --html

bloat:
    cargo bloat --release --crates

# UC 06 — hermetic shell-script tests for fetch-yt-dlp.sh.
# Requires `bats-core` on PATH (`brew install bats-core` /
# `apt install bats` on Ubuntu 22.04+ / `cargo install bats` is NOT a thing).
test-scripts:
    bats scripts/test-fetch-yt-dlp.bats

# UC 17 — documented escape hatch for runtime-deps fetch. Pulls
# ffmpeg into runtime-deps/ for the host's target so `cargo run` can
# invoke yt-dlp with `--ffmpeg-location` without a system-wide install.
#
# Pin source: scripts/runtime-deps-pins.env (sourced by both shells).
# Linux/Windows: BtbN/FFmpeg-Builds LGPL-only, SHA256-pinned.
# macOS: built from upstream FFmpeg source via build-ffmpeg-macos.sh
# (no LGPL-only mainstream macOS build exists; evermeet.cx ships only
# x86_64). The macOS build takes ~10–15 minutes the first time.
fetch-runtime-deps:
    #!/usr/bin/env bash
    set -euo pipefail
    HOST="$(rustc -vV | sed -n 's/^host: //p')"
    mkdir -p runtime-deps
    case "${HOST}" in
        *apple-darwin)
            bash scripts/build-ffmpeg-macos.sh runtime-deps/
            ;;
        *)
            bash scripts/fetch-ffmpeg.sh "${HOST}" runtime-deps/
            ;;
    esac

# UC 13 — regenerate the icon snapshot baseline.
# Branch A is active: the `icon_snapshot_test` honors SNAPSHOT_UPDATE=1 by
# overwriting `crates/app/tests/baselines/_component_smoke.png` with the
# freshly rendered PNG instead of pixel-diffing against it. Re-run after any
# legitimate visual change to the icon-bearing samples in `_ComponentSmoke`,
# inspect the resulting PNG, and commit it.
#
# Branch B alternative (manual `slint-viewer` baseline) is documented in
# docs/adr/0009-icon-fidelity-and-snapshot-tests.md and is NOT wired up here.
snapshot-update:
    SNAPSHOT_UPDATE=1 cargo test --package app --test icon_snapshot_test
