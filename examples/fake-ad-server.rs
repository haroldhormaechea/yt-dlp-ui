//! Fake ad server — local dev only.
//!
//! Serves a static placeholder image and a fake click-through endpoint so
//! contributors can exercise the consent + ad-window flow without a real
//! ad-vendor account.
//!
//! Run via:
//!     cargo run --example fake-ad-server
//!
//! Then point the `ad-window` IPC `show` command at:
//!     <http://127.0.0.1:7733/ad>
//!
//! `axum` is a `[dev-dependencies]` of the `app` crate, so this example never
//! ships in release builds.

use axum::{
    Router,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect},
    routing::get,
};

const BIND_ADDR: &str = "127.0.0.1:7733";

/// 1x1 transparent PNG — base64 of the canonical smallest-possible PNG.
/// Decoded to 67 bytes at startup; embedded so this example needs no
/// auxiliary files.
const PLACEHOLDER_PNG_B64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(landing))
        .route("/ad", get(ad_html))
        .route("/placeholder.png", get(placeholder_png))
        .route("/click", get(click_through));

    let listener = tokio::net::TcpListener::bind(BIND_ADDR)
        .await
        .expect("bind 127.0.0.1:7733 (is the port already in use?)");

    println!("fake-ad-server listening on http://{BIND_ADDR}");
    println!("  GET /            — sanity landing page");
    println!("  GET /ad          — ad HTML wrapping the placeholder image");
    println!("  GET /placeholder.png — 1x1 transparent PNG");
    println!("  GET /click       — fake click-through (302 to example.com)");

    axum::serve(listener, app)
        .await
        .expect("axum::serve failed");
}

async fn landing() -> &'static str {
    "fake-ad-server is up. See examples/fake-ad-server.rs for endpoints.\n"
}

async fn ad_html() -> impl IntoResponse {
    let html = r#"<!doctype html>
<html>
  <head><title>Sponsor placeholder</title></head>
  <body style="margin:0;padding:0;background:#222;color:#eee;font-family:system-ui">
    <a href="/click">
      <img src="/placeholder.png" alt="sponsor placeholder" style="width:100%;height:auto" />
      <p style="text-align:center;padding:8px">Your ad here — local dev only</p>
    </a>
  </body>
</html>"#;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    (StatusCode::OK, headers, html)
}

async fn placeholder_png() -> impl IntoResponse {
    let bytes = base64_decode(PLACEHOLDER_PNG_B64);
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (StatusCode::OK, headers, bytes)
}

async fn click_through() -> impl IntoResponse {
    Redirect::to("https://example.com/")
}

/// Tiny base64 decoder — avoids pulling in the `base64` crate just for one
/// constant. Implements the standard alphabet only, with `=` padding.
#[allow(clippy::cast_possible_truncation)]
fn base64_decode(input: &str) -> Vec<u8> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &c) in TABLE.iter().enumerate() {
        // i is in 0..64 by construction (TABLE has 64 entries) — safe to cast.
        lookup[c as usize] = i as u8;
    }

    let bytes: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();

    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for b in bytes {
        let v = lookup[b as usize];
        assert!(v < 64, "invalid base64 input");
        buf = (buf << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            // `bits` < 8 here; (buf >> bits) fits in a u8.
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    out
}
