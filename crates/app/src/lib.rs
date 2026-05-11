//! `yt-dlp-ui` application library.
//!
//! Owns the runtime entry point [`run`], the database layer ([`db`]), the
//! domain model ([`model`]), filesystem path helpers ([`paths`]), the logging
//! setup ([`logging`]), and the download manager ([`download_mgr`]).
//!
//! `main.rs` is a 3-line shim that calls [`run`] — keeping the entry point in
//! a library lets integration tests import the same code path.

pub mod about;
pub mod bot_check;
pub mod browsers;
mod build_paths;
pub mod db;
pub mod download_mgr;
pub mod error;
pub mod formats;
pub mod logging;
pub mod model;
pub mod paths;
pub mod thumbnails;
pub mod ui_bridge;
pub mod url_open;

pub use error::AppError;

/// Placeholder for the eventual ad-vendor's privacy-policy URL. UC 09
/// sets the corresponding Slint property to this string at startup; an
/// empty value drives the disabled-link branch (AC#18). Wire a real URL
/// here when the ad vendor is selected.
pub const VENDOR_PRIVACY_URL: &str = "";

use std::path::Path;

use tokio::sync::mpsc;

use crate::browsers::Browser;
use crate::db::{Db, queue, settings};
use crate::download_mgr::{DownloadManager, RealBridge, UiEvent};

slint::include_modules!();

/// Main application entry point.
///
/// Flow (per `PROJECT_BRIEF.md` § Architecture and the approved proposal):
/// 1. Resolve `app_data_dir`; create if missing.
/// 2. Initialize logging; hold the [`tracing_appender::non_blocking::WorkerGuard`]
///    for the lifetime of this function.
/// 3. Open the `SQLite` DB and run migrations.
/// 4. Revert any `in_flight` rows back to `queued` (with progress fields zeroed).
/// 5. Build the tokio runtime; build the [`DownloadManager`].
/// 6. Re-issue title fetches for any `pending` / `fetching` rows.
/// 7. **Smoke-mode short-circuit:** if `YT_DLP_UI_SMOKE` is set in the
///    environment, log a structured `smoke_ok` line and return — useful for CI
///    "does the binary even start" gates without paying the Slint init cost.
/// 8. Otherwise, build the Slint window, wire callbacks, run the event loop.
///
/// # Errors
///
/// Any [`AppError`] variant — see the type's docs.
pub fn run() -> Result<(), AppError> {
    // 1. App-data dir.
    let app_data = paths::app_data_dir()?;
    std::fs::create_dir_all(&app_data)?;

    // 2. Logging — keep the guard alive across the whole function.
    let _log_guard = logging::init(&app_data)?;

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        bridge_version = yt_dlp_bridge::version(),
        "yt-dlp-ui starting"
    );

    // 3. DB.
    let db_path = app_data.join("db.sqlite");
    let db = Db::open(&db_path)?;

    // 4. Revert in_flight → queued.
    let reverted = db.with_conn(queue::revert_in_flight_to_queued)?;
    if reverted > 0 {
        tracing::info!(count = reverted, "reverted in-flight rows to queued");
    }

    // Resolve default download dir up front so settings reads have a fallback.
    let default_dl = paths::default_download_dir()?;
    let dest_dir = db.with_conn(|c| settings::get_dest_dir(c, &default_dl))?;
    if !dest_dir.exists()
        && let Err(err) = std::fs::create_dir_all(&dest_dir)
    {
        tracing::warn!(?err, dest_dir = %dest_dir.display(), "failed to create dest dir");
    }

    let cap = db.with_conn(settings::get_concurrency_cap)?;

    // 5. Tokio runtime.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| AppError::Runtime(e.to_string()))?;

    // 6. Bridge + manager + UI event channel.
    // UC 17: strip the macOS quarantine attribute from yt-dlp / ffmpeg /
    // deno before any subprocess spawn so Gatekeeper does not prompt the
    // user once per auxiliary binary.
    paths::strip_macos_quarantine_if_needed();

    let yt_dlp_path = paths::bundled_yt_dlp_path()?;
    tracing::info!(yt_dlp = %yt_dlp_path.display(), "resolved yt-dlp binary");
    let bridge = RealBridge::new(yt_dlp_path);
    let (ui_tx, ui_rx) = mpsc::channel::<UiEvent>(64);

    let detected = crate::browsers::detect_installed(None);
    tracing::info!(
        count = detected.len(),
        "detected browsers for cookies dialog"
    );
    let deno_path = paths::resolved_deno_path();
    if let Some(p) = &deno_path {
        tracing::info!(deno = %p.display(), "resolved deno binary");
    }
    // UC 17: ffmpeg is required for YouTube DASH merge. Type discipline
    // forces explicit BundledMissing handling here. Downgrade to Option at
    // this single call site so downstream code carries `Option<PathBuf>`
    // and the manager's spawn-time gate can refuse to start a download
    // with a user-visible error message instead of crashing the app.
    let ffmpeg_path = match paths::bundled_ffmpeg_path() {
        Ok(p) => {
            tracing::info!(ffmpeg = %p.display(), "resolved ffmpeg binary");
            Some(p)
        }
        Err(err) => {
            tracing::warn!(
                ?err,
                "ffmpeg unavailable; downloads requiring merge will error"
            );
            None
        }
    };

    // UC 28: ffprobe is required for yt-dlp's audio-only ExtractAudio
    // post-processor and several metadata-probing paths. The bridge passes
    // `--ffmpeg-location <parent_dir>` and yt-dlp discovers both binaries
    // from that directory, so the staging path is what determines runtime
    // behaviour — we don't need a new bridge flag. We log resolution here
    // for diagnostic value (matches ffmpeg's pattern) but don't gate spawn
    // on it: yt-dlp surfaces a runtime "ffprobe not found" error if the
    // binary is missing at the configured directory, which is the user-
    // visible signal we want.
    let ffprobe_path = match paths::bundled_ffprobe_path() {
        Ok(p) => {
            tracing::info!(ffprobe = %p.display(), "resolved ffprobe binary");
            Some(p)
        }
        Err(err) => {
            tracing::warn!(
                ?err,
                "ffprobe unavailable; audio-only downloads and metadata probing may error"
            );
            None
        }
    };

    // UC 28 co-location invariant: yt-dlp's `--ffmpeg-location <dir>`
    // discovers both `ffmpeg` and `ffprobe` from a single directory. When
    // both binaries resolve, their parents MUST be equal — otherwise the
    // directory we pass to yt-dlp can only point at one of them and the
    // other will be reported "not found". An invariant violation indicates
    // a packaging or path-resolver bug; log ERROR for diagnostic but don't
    // block startup — yt-dlp will surface the runtime failure on the
    // affected operations and the log line links the user-visible error
    // to the staging mismatch.
    if let (Some(ffm), Some(ffp)) = (ffmpeg_path.as_ref(), ffprobe_path.as_ref())
        && let (Some(ffm_dir), Some(ffp_dir)) = (ffm.parent(), ffp.parent())
        && ffm_dir != ffp_dir
    {
        tracing::error!(
            ffmpeg = %ffm.display(),
            ffprobe = %ffp.display(),
            "ffmpeg and ffprobe are not co-located; yt-dlp --ffmpeg-location can only point at one directory and the other binary will be reported missing at runtime"
        );
    }
    // The bridge picks ffprobe up via yt-dlp's `--ffmpeg-location <parent_dir>`
    // flag (no separate ffprobe-path plumbing into DownloadRequest — see
    // UC 28 § "Bridge — no API change"). `ffprobe_path` is bound for the
    // co-location check + diagnostic logging only; it drops at function end.

    // UC 08: per-row thumbnail cache lives at <app-data>/thumbnails/. Created
    // lazily on first fetch by the `thumbnails` module; we just compute the
    // path here.
    let thumbnail_cache_dir = app_data.join("thumbnails");

    let manager = runtime.block_on(async {
        DownloadManager::new(
            db.clone(),
            bridge,
            ui_tx,
            cap,
            detected.clone(),
            deno_path.clone(),
            ffmpeg_path.clone(),
            thumbnail_cache_dir,
        )
    });

    // Re-issue any pending title fetches now that the runtime is up.
    runtime.block_on(async {
        if let Err(err) = manager.requeue_pending_title_fetches().await {
            tracing::warn!(?err, "failed to re-issue pending title fetches");
        }
    });

    // UC 08: same for per-row thumbnail fetches that did not complete
    // before the previous shutdown.
    runtime.block_on(async {
        if let Err(err) = manager.requeue_pending_thumbnail_fetches().await {
            tracing::warn!(?err, "failed to re-issue pending thumbnail fetches");
        }
    });

    // 7. Smoke-mode short-circuit.
    if std::env::var("YT_DLP_UI_SMOKE").is_ok() {
        let initial_rows = runtime.block_on(async { manager.list_ui_rows().await });
        let count = initial_rows.as_ref().map(Vec::len).unwrap_or_default();
        tracing::info!(loaded_queue_count = count, "smoke_ok");
        return Ok(());
    }

    // 8. Build & run the Slint window.
    run_ui(
        &runtime,
        &db,
        &manager,
        ui_rx,
        &default_dl,
        &detected,
        deno_path.is_some(),
    )?;
    drop(runtime);
    Ok(())
}

fn run_ui(
    runtime: &tokio::runtime::Runtime,
    db: &Db,
    manager: &DownloadManager<RealBridge>,
    ui_rx: mpsc::Receiver<UiEvent>,
    default_dl: &Path,
    detected: &[Browser],
    deno_resolved: bool,
) -> Result<(), AppError> {
    let window = MainWindow::new().map_err(|e| AppError::Ui(e.to_string()))?;

    // Initial paint: load existing rows from the DB.
    let initial = runtime
        .block_on(async { manager.list_ui_rows().await })
        .unwrap_or_default();
    let model = std::rc::Rc::new(slint::VecModel::<QueueRow>::from(
        initial.into_iter().map(to_slint_row).collect::<Vec<_>>(),
    ));
    window.set_queue(model.into());

    // UC 08: seed the footer's `cap` from settings; counts get computed
    // after the model is built (and on every event by `recompute_counts`).
    let cap = db
        .with_conn(settings::get_concurrency_cap)
        .map_or(3, |c| i32::try_from(c).unwrap_or(3));
    window.set_cap(cap);

    let dest_path = db
        .with_conn(|c| settings::get_dest_dir(c, default_dl))
        .unwrap_or_else(|_| default_dl.to_path_buf());
    window.set_dest_dir_display(slint::SharedString::from(formats::format_dest_dir(
        &dest_path,
    )));

    let format_string = db
        .with_conn(settings::get_format_pref)
        .map_or("BestHeuristic", format_pref_to_str);
    window.set_format_pref(slint::SharedString::from(format_string));

    // UC 09: seed concurrency cap, focus-mode, ads-personalized, cookies-empty,
    // vendor-privacy-url so the settings panel reflects persisted state at
    // first paint.
    let cap_for_panel = db
        .with_conn(settings::get_concurrency_cap)
        .map_or(3, |c| i32::try_from(c).unwrap_or(3));
    window.set_concurrency_cap(cap_for_panel);

    let focus_mode = db.with_conn(settings::get_focus_mode).unwrap_or(false);
    window.set_focus_mode(focus_mode);

    let ads_personalized = db.with_conn(settings::get_ads_personalized).unwrap_or(true);
    window.set_ads_personalized(ads_personalized);

    window.set_cookies_empty(detected.is_empty());
    window.set_vendor_privacy_url(slint::SharedString::from(VENDOR_PRIVACY_URL));

    // Cookies-source dropdown — populate from detected browsers using
    // user-facing display names (UC 09). A leading "None" entry stands for
    // the no-cookies default.
    let mut cookies_options: Vec<slint::SharedString> = Vec::with_capacity(detected.len() + 1);
    cookies_options.push(slint::SharedString::from("None"));
    for b in detected {
        cookies_options.push(slint::SharedString::from(b.display_name()));
    }
    let options_model = std::rc::Rc::new(slint::VecModel::<slint::SharedString>::from(
        cookies_options,
    ));
    window.set_cookies_options(options_model.into());
    let current_cookies = db
        .with_conn(settings::get_cookies_browser)
        .ok()
        .flatten()
        .map_or("None", |b| b.display_name());
    window.set_cookies_browser(slint::SharedString::from(current_cookies));

    // Bot-check dialog list — pre-populate with the same detected set, also
    // using user-facing display names so the popup reads "Brave / Chrome /
    // …" instead of yt-dlp's lowercase argument tokens (UC 09 side benefit).
    let popup_model = std::rc::Rc::new(slint::VecModel::<slint::SharedString>::from(
        detected
            .iter()
            .map(|b| slint::SharedString::from(b.display_name()))
            .collect::<Vec<_>>(),
    ));
    window.set_bot_check_options(popup_model.into());

    // UC 10: seed the bot-check modal's transient state at startup so the
    // first open of a session has well-defined defaults. The bridge's
    // ShowBotCheckDialog handler overwrites `bot_check_default_pick` and
    // the per-browser flags before raising `bot_check_open`; these seeds
    // exist so the Slint properties have valid initial values.
    window.set_bot_check_affected_count(0);
    window.set_bot_check_default_pick(slint::SharedString::from(""));
    window.set_bot_check_last_pick(slint::SharedString::from(""));

    // Deno banner: show whenever the deno probe found neither bundled deno
    // nor PATH deno. Dismissal is session-only (UC 11) — the banner reappears
    // next launch if the probe still fails.
    window.set_deno_warning_visible(!deno_resolved);

    // UC 18: seed the About modal — version string + bundled-software
    // entries. The slice-of-statics from `about::entries()` is mapped to
    // the Slint-generated `AboutEntry` struct; `source_notice: None` is
    // flattened to an empty `SharedString` because Slint structs have no
    // Option type.
    window.set_app_version(slint::SharedString::from(about::APP_VERSION));
    let about_entries: Vec<AboutEntry> = about::entries()
        .iter()
        .map(|e| AboutEntry {
            name: slint::SharedString::from(e.name),
            version: slint::SharedString::from(e.version),
            license_name: slint::SharedString::from(e.license_name),
            license_text: slint::SharedString::from(e.license_text),
            source_notice: slint::SharedString::from(e.source_notice.unwrap_or("")),
        })
        .collect();
    window.set_about_entries(slint::ModelRc::new(slint::VecModel::from(about_entries)));

    // UC 11: seed the toasts model with an empty VecModel so the ui_bridge
    // can downcast and push entries (mirrors the cookies-options seeding
    // above). The Slint default for an unbound `[ToastEntry]` property is
    // a non-VecModel sentinel which the downcast in `push_toast_on_main`
    // would fail.
    window.set_toasts(slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::<
        ToastEntry,
    >::default())));

    // Seed the theme palette before any callbacks fire so the first paint
    // already reflects the persisted (or system-derived) dark/light choice.
    ui_bridge::seed_theme_from_settings(&window, db);

    // Wire callbacks.
    ui_bridge::wire_callbacks(&window, &runtime.handle().clone(), manager, db);

    // Spawn the UI bridge task.
    let weak = window.as_weak();
    runtime.spawn(async move {
        ui_bridge::run_ui_bridge(weak, ui_rx).await;
    });

    window.run().map_err(|e| AppError::Ui(e.to_string()))?;
    Ok(())
}

/// Converts a `UiQueueRow` to the Slint-generated `QueueRow` struct.
fn to_slint_row(row: model::UiQueueRow) -> QueueRow {
    let speed = row
        .speed_bps
        .map(|b| format!("{b} B/s"))
        .unwrap_or_default();
    let eta = row.eta_s.map(|s| format!("{s} s")).unwrap_or_default();
    // UC 08: project the on-disk thumbnail to a Slint Image; gradient
    // placeholder remains until the loaded flag flips. `Image::default()`
    // is a 0×0 sentinel; `thumbnail-loaded` carries the explicit signal.
    let (thumbnail_image, thumbnail_loaded) = match &row.thumbnail_path {
        Some(path) => slint::Image::load_from_path(path)
            .map_or_else(|_| (slint::Image::default(), false), |img| (img, true)),
        None => (slint::Image::default(), false),
    };
    let seed_val = i32::try_from(thumbnails::deterministic_seed(&row.url) % 8).unwrap_or(0);
    let source_kind = thumbnails::source_kind_from_url(&row.url);
    let dest_display = formats::format_dest_dir(&row.dest_dir);
    let size_text = formats::format_size(row.size_bytes);
    let downloaded_text = formats::format_size(row.downloaded_bytes);

    QueueRow {
        id: i32::try_from(row.id).unwrap_or(i32::MAX),
        url: row.url.into(),
        title: row.title.into(),
        title_status: row.title_status.as_str().into(),
        status: row.status.as_str().into(),
        progress_pct: row.progress_pct,
        speed: speed.into(),
        eta: eta.into(),
        error_msg: row.error_msg.unwrap_or_default().into(),
        waiting_on_user: false,
        size: size_text.into(),
        downloaded: downloaded_text.into(),
        dest_dir_display: dest_display.into(),
        source: source_kind.into(),
        seed: seed_val,
        thumbnail_path: thumbnail_image,
        thumbnail_loaded,
    }
}

fn format_pref_to_str(pref: yt_dlp_bridge::FormatPref) -> &'static str {
    match pref {
        yt_dlp_bridge::FormatPref::BestVideo => "BestVideo",
        yt_dlp_bridge::FormatPref::BestAudioMp3 => "BestAudioMp3",
        yt_dlp_bridge::FormatPref::BestAudioOpus => "BestAudioOpus",
        yt_dlp_bridge::FormatPref::BestAudioM4a => "BestAudioM4a",
        yt_dlp_bridge::FormatPref::BestHeuristic => "BestHeuristic",
    }
}

// `BestAudioM4a` is intentionally NOT exposed in the Settings dropdown (per
// UC 19 AC #1: per-URL only). The dropdown calls `format_pref_from_str` and
// falls back to `BestHeuristic` for unknown strings.
/// Maps a string back to a [`yt_dlp_bridge::FormatPref`]. Unknown strings
/// default to [`yt_dlp_bridge::FormatPref::BestHeuristic`].
#[must_use]
pub fn format_pref_from_str(s: &str) -> yt_dlp_bridge::FormatPref {
    match s {
        "BestVideo" => yt_dlp_bridge::FormatPref::BestVideo,
        "BestAudioMp3" => yt_dlp_bridge::FormatPref::BestAudioMp3,
        "BestAudioOpus" => yt_dlp_bridge::FormatPref::BestAudioOpus,
        "BestAudioM4a" => yt_dlp_bridge::FormatPref::BestAudioM4a,
        _ => yt_dlp_bridge::FormatPref::BestHeuristic,
    }
}

/// Maps the `AddBar`'s "Audio only" toggle state to a per-URL `FormatPref`
/// override. UC 19: `true` → `BestAudioM4a`; `false` → `None` (caller falls
/// back to the Settings-default `format_pref`).
#[must_use]
pub fn format_pref_from_audio_only_flag(audio_only: bool) -> Option<yt_dlp_bridge::FormatPref> {
    if audio_only {
        Some(yt_dlp_bridge::FormatPref::BestAudioM4a)
    } else {
        None
    }
}

/// Public re-export so integration tests can refer to the conversion.
#[must_use]
pub fn ui_row_for_test(row: model::UiQueueRow) -> QueueRow {
    to_slint_row(row)
}

/// Returns the path to the `db.sqlite` file inside `app_data_dir`. Helper
/// shared with integration tests that need to seed the DB before starting
/// the app.
#[must_use]
pub fn db_path_for(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("db.sqlite")
}
