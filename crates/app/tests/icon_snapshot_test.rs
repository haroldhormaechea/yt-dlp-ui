//! UC 13 — icon-fidelity snapshot test.
//!
//! Renders `_ComponentSmoke` into an in-memory RGBA buffer via Slint's
//! software renderer (Branch A in `docs/adr/0009-icon-fidelity-and-snapshot-tests.md`),
//! encodes it as PNG, and pixel-diffs against the committed baseline at
//! `crates/app/tests/baselines/_component_smoke.png`. The baseline guards
//! the design-system regression contract from the use case (AC #9).
//!
//! macOS-only: sub-pixel AA / font-rasterization differences across OSes
//! make multi-OS pixel diffs unstable without per-OS baselines, which is a
//! research project of its own (AC #13). The dev-deps that pull in the
//! `renderer-software` Slint feature and the `image` crate are
//! cfg-gated to `cfg(target_os = "macos")` in `crates/app/Cargo.toml`,
//! and so is this test file.
//!
//! Set `SNAPSHOT_UPDATE=1` in the env to overwrite the baseline instead of
//! comparing against it (AC #11). The `Justfile`'s `snapshot-update`
//! recipe wires this knob.
//!
//! UC 29 (2026-05-11): `HEIGHT` was raised from 380 to 460 px to match
//! `_ComponentSmoke`'s new Window height after two `AddBar` instances were
//! added at the head of its `VerticalLayout` (placeholder-state + typed-text
//! state — the AC #10 regression-resistance check). Capturing the full
//! Window is required so a future markup change to the lower rows does not
//! silently fall outside the rendered frame. The baseline PNG at
//! `crates/app/tests/baselines/_component_smoke.png` must be regenerated on
//! macOS via `SNAPSHOT_UPDATE=1 cargo test --package app --test
//! icon_snapshot_test` and committed alongside the UC 29 fix.

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::rc::Rc;

use slint::ComponentHandle;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter};

// Pull in `_ComponentSmoke`, re-exported from `main_window.slint`.
slint::include_modules!();

const WIDTH: u32 = 480;
// UC 29: bumped 380 → 460 to match `_ComponentSmoke`'s new Window height
// (two `AddBar` instances added at the head of its `VerticalLayout`).
const HEIGHT: u32 = 460;

struct TestPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
    // duration_since_start, click_interval, cursor_flash_cycle, clipboard,
    // event-loop methods all use trait defaults — a single render pass does
    // not need an event loop. ADR 0009 § "Snapshot-test approach (Branch A)".
}

fn baseline_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("baselines");
    p.push("_component_smoke.png");
    p
}

fn actual_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("baselines");
    p.push("_component_smoke.actual.png");
    p
}

#[test]
fn component_smoke_pixel_matches_baseline() {
    // Install the software-renderer-backed test platform. `set_platform`
    // returns Err if a backend is already set (e.g., another test in the
    // binary has run first). With a single test in this crate-test binary
    // that shouldn't happen, but if it does, fall through and try to render
    // anyway — `_ComponentSmoke::new()` will surface a clear error if the
    // platform is unusable.
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    let _ = slint::platform::set_platform(Box::new(TestPlatform {
        window: window.clone(),
    }));

    let smoke = match _ComponentSmoke::new() {
        Ok(s) => s,
        Err(err) => {
            // No platform / display surface at all. Pin the failure mode
            // visibly rather than passing silently — this test exists
            // specifically to catch icon regressions, so a no-op is worse
            // than a test failure.
            panic!("snapshot test cannot construct _ComponentSmoke: {err}");
        }
    };

    // The component is a `Window` rooted at 480 × 460 px (UC 29 raised
    // the height from 380 to 460 to accommodate the two new `AddBar`
    // instances at the head of the smoke's VerticalLayout); size the
    // software window adapter to match so the renderer paints the full
    // surface. Physical size — the test renders at scale factor 1.0.
    window.set_size(slint::PhysicalSize::new(WIDTH, HEIGHT));
    smoke.window().request_redraw();

    let mut buffer: Vec<slint::Rgb8Pixel> =
        vec![slint::Rgb8Pixel { r: 0, g: 0, b: 0 }; (WIDTH as usize) * (HEIGHT as usize)];
    let stride = WIDTH as usize;

    let drew = window.draw_if_needed(|renderer| {
        renderer.render(buffer.as_mut_slice(), stride);
    });
    assert!(
        drew,
        "software renderer reported no draw — _ComponentSmoke did not paint"
    );

    // RGB → RGBA so we can hand the buffer to the `image` crate's PNG
    // encoder via `RgbaImage::from_raw`. Software renderer paints opaque
    // pixels, so a hard-coded alpha of 255 is correct.
    let mut rgba: Vec<u8> = Vec::with_capacity(buffer.len() * 4);
    for px in &buffer {
        rgba.push(px.r);
        rgba.push(px.g);
        rgba.push(px.b);
        rgba.push(255);
    }
    let actual = image::RgbaImage::from_raw(WIDTH, HEIGHT, rgba)
        .expect("RGBA buffer length matches WIDTH * HEIGHT * 4");

    let baseline = baseline_path();
    if std::env::var_os("SNAPSHOT_UPDATE").is_some() {
        if let Some(parent) = baseline.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("create baseline dir {}: {e}", parent.display()));
        }
        actual
            .save(&baseline)
            .unwrap_or_else(|e| panic!("write baseline {}: {e}", baseline.display()));
        eprintln!(
            "SNAPSHOT_UPDATE=1 — wrote baseline at {}",
            baseline.display()
        );
        return;
    }

    let expected_bytes = match std::fs::read(&baseline) {
        Ok(b) => b,
        Err(err) => {
            // Surface the actual render so the user can eyeball it and
            // promote it to the baseline if it looks correct.
            let actual_dst = actual_path();
            if let Some(parent) = actual_dst.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = actual.save(&actual_dst);
            panic!(
                "baseline missing at {}: {err}. Actual render written to {}. \
                 Run `just snapshot-update` (or `SNAPSHOT_UPDATE=1 cargo test \
                 --package app --test icon_snapshot_test`) to commit the baseline.",
                baseline.display(),
                actual_dst.display()
            );
        }
    };
    let expected = image::load_from_memory(&expected_bytes)
        .unwrap_or_else(|e| panic!("decode baseline {}: {e}", baseline.display()))
        .to_rgba8();

    if expected.dimensions() != actual.dimensions() {
        let actual_dst = actual_path();
        let _ = actual.save(&actual_dst);
        panic!(
            "baseline dimensions {:?} != actual {:?}. Actual render written to {}.",
            expected.dimensions(),
            actual.dimensions(),
            actual_dst.display()
        );
    }

    if expected.as_raw() != actual.as_raw() {
        let actual_dst = actual_path();
        let _ = actual.save(&actual_dst);
        let mismatched = expected
            .pixels()
            .zip(actual.pixels())
            .filter(|(a, b)| a != b)
            .count();
        panic!(
            "icon snapshot diverged from baseline at {} ({} pixels differ). \
             Actual render written to {}. If the change is intended, re-run \
             `just snapshot-update` and commit the new baseline.",
            baseline.display(),
            mismatched,
            actual_dst.display()
        );
    }
}
