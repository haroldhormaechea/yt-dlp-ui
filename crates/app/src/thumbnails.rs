//! Thumbnail pipeline — deterministic seed for the gradient placeholder,
//! hostname-derived source kind for the corner glyph, and the per-row
//! background fetcher that downloads the upstream thumbnail and caches it
//! to `<app-data>/thumbnails/<sha1(url)>.<ext>`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use thiserror::Error;

// QA owns `thumbnails_test.rs` next to this file. The include line is
// added here once that file lands; until then, leaving it out keeps
// `cargo fmt` from tripping on a missing module path.

/// Timeout for a single thumbnail HTTP fetch. Generous because thumbnail
/// CDNs can be flaky, but bounded so a stuck request doesn't pin a tokio
/// worker. Failures are non-fatal — the gradient placeholder remains.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Maximum bytes a fetched thumbnail may have. Keeps a malicious or
/// misbehaving CDN from filling the user's disk; 8 MiB is generous for
/// per-video JPEG/WEBP thumbnails (typical: 50-300 KB).
const MAX_THUMBNAIL_BYTES: u64 = 8 * 1024 * 1024;

/// Errors raised by [`fetch_and_cache_thumbnail`]. Always logged at WARN
/// by the caller; the row keeps its gradient placeholder.
#[derive(Debug, Error)]
pub enum ThumbnailError {
    #[error("http request failed: {0}")]
    Http(String),
    #[error("thumbnail too large: {0} bytes (max {MAX_THUMBNAIL_BYTES})")]
    TooLarge(u64),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

/// Returns a stable `u32` seed derived from the row's URL. The 8 gradient
/// palettes in `thumbnail.slint` are indexed by `seed % 8`, so `(seed % 8)
/// as i32` is the value that gets handed to Slint.
///
/// Algorithm: 32-bit FNV-like rolling hash. Cheap (one pass over the bytes),
/// no `std::collections::hash_map::DefaultHasher` dependency (which is not
/// guaranteed stable across compiler versions).
#[must_use]
pub fn deterministic_seed(url: &str) -> u32 {
    let mut h: u32 = 0;
    for b in url.bytes() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(b));
    }
    h
}

/// Maps a URL hostname to a known source kind, falling back to `"globe"`
/// for everything else. The string is the exact value the Slint row passes
/// into `SourceIcon::source-kind`.
#[must_use]
pub fn source_kind_from_url(url: &str) -> String {
    let lower = url.to_lowercase();
    if lower.contains("youtube.com") || lower.contains("youtu.be") {
        return "youtube".to_string();
    }
    if lower.contains("vimeo.com") {
        return "vimeo".to_string();
    }
    if lower.contains("soundcloud.com") {
        return "soundcloud".to_string();
    }
    if lower.contains("bandcamp.com") {
        return "bandcamp".to_string();
    }
    "globe".to_string()
}

/// Computes the per-row cache file path. Hex-encodes a SHA-1 of the URL so
/// the filename is deterministic and filesystem-safe across all OSes;
/// preserves the upstream extension (`.jpg`, `.webp`) when one can be
/// inferred from the URL or `Content-Type`.
#[must_use]
pub fn cache_path(cache_dir: &Path, url: &str, ext: &str) -> PathBuf {
    let digest = sha1_hex(url.as_bytes());
    let safe_ext = if ext.is_empty() { "jpg" } else { ext };
    cache_dir.join(format!("{digest}.{safe_ext}"))
}

/// Downloads the upstream thumbnail at `url`, writes it to `<cache_dir>/<sha1>.<ext>`,
/// and returns the resulting path. Caller persists the path via
/// `queue::set_thumbnail_path` and emits a `UiEvent` so the row crossfades.
///
/// # Errors
///
/// Returns [`ThumbnailError`] for HTTP failures, oversize responses, or
/// filesystem errors. None are fatal — the calling task logs at WARN and
/// the row keeps its gradient placeholder.
pub async fn fetch_and_cache_thumbnail(
    url: &str,
    cache_dir: &Path,
) -> Result<PathBuf, ThumbnailError> {
    tokio::fs::create_dir_all(cache_dir).await?;

    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(|e| ThumbnailError::Http(e.to_string()))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| ThumbnailError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(ThumbnailError::Http(format!(
            "non-success status: {}",
            resp.status()
        )));
    }

    let ext_from_ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(infer_ext_from_content_type)
        .unwrap_or_default();
    let ext = if ext_from_ct.is_empty() {
        infer_ext_from_url(url)
    } else {
        ext_from_ct
    };

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ThumbnailError::Http(e.to_string()))?;
    let len = bytes.len() as u64;
    if len > MAX_THUMBNAIL_BYTES {
        return Err(ThumbnailError::TooLarge(len));
    }

    let path = cache_path(cache_dir, url, &ext);
    tokio::fs::write(&path, &bytes).await?;
    Ok(path)
}

fn infer_ext_from_content_type(ct: &str) -> String {
    let lower = ct.to_ascii_lowercase();
    if lower.contains("image/jpeg") || lower.contains("image/jpg") {
        return "jpg".to_string();
    }
    if lower.contains("image/png") {
        return "png".to_string();
    }
    if lower.contains("image/webp") {
        return "webp".to_string();
    }
    if lower.contains("image/gif") {
        return "gif".to_string();
    }
    String::new()
}

fn infer_ext_from_url(url: &str) -> String {
    // Strip query string, then take the last segment's extension if any.
    let no_query = url.split('?').next().unwrap_or(url);
    let last = no_query.rsplit('/').next().unwrap_or("");
    if let Some(idx) = last.rfind('.') {
        let ext = &last[idx + 1..];
        if !ext.is_empty() && ext.len() <= 5 && ext.chars().all(char::is_alphanumeric) {
            return ext.to_ascii_lowercase();
        }
    }
    "jpg".to_string()
}

/// Inline SHA-1 (FIPS 180-4). Used to derive a stable on-disk filename
/// from the row's URL — not security-sensitive. Single call site, so a
/// dedicated `sha1` crate dependency would be overkill.
#[allow(clippy::similar_names, clippy::many_single_char_names)]
fn sha1_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut state: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let bit_len: u64 = bytes.len() as u64 * 8;
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks(64) {
        let mut w = [0u32; 80];
        for (i, slot) in w.iter_mut().enumerate().take(16) {
            let off = i * 4;
            *slot =
                u32::from_be_bytes([chunk[off], chunk[off + 1], chunk[off + 2], chunk[off + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let mut aa = state[0];
        let mut bb = state[1];
        let mut cc = state[2];
        let mut dd = state[3];
        let mut ee = state[4];
        for (i, w_i) in w.iter().enumerate() {
            let (fn_v, kk) = if i < 20 {
                ((bb & cc) | ((!bb) & dd), 0x5A82_7999)
            } else if i < 40 {
                (bb ^ cc ^ dd, 0x6ED9_EBA1)
            } else if i < 60 {
                ((bb & cc) | (bb & dd) | (cc & dd), 0x8F1B_BCDC)
            } else {
                (bb ^ cc ^ dd, 0xCA62_C1D6)
            };
            let temp = aa
                .rotate_left(5)
                .wrapping_add(fn_v)
                .wrapping_add(ee)
                .wrapping_add(kk)
                .wrapping_add(*w_i);
            ee = dd;
            dd = cc;
            cc = bb.rotate_left(30);
            bb = aa;
            aa = temp;
        }
        state[0] = state[0].wrapping_add(aa);
        state[1] = state[1].wrapping_add(bb);
        state[2] = state[2].wrapping_add(cc);
        state[3] = state[3].wrapping_add(dd);
        state[4] = state[4].wrapping_add(ee);
    }

    let mut out = String::with_capacity(40);
    for word in &state {
        for byte in &word.to_be_bytes() {
            let _ = write!(out, "{byte:02x}");
        }
    }
    out
}
