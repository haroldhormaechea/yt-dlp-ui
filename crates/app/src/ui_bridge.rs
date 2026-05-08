//! Bridge between the tokio runtime and the Slint UI.
//!
//! This is the single touchpoint where the runtime mutates Slint state.
//! Everything else stays decoupled from the UI: the download manager
//! emits [`UiEvent`]s on an `mpsc::Receiver`; this module forwards them to
//! the Slint event loop via `slint::invoke_from_event_loop`.

use std::path::PathBuf;

use slint::{ComponentHandle, Model, SharedString, VecModel, Weak};
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use crate::browsers::Browser;
use crate::db::settings::{ExplicitTheme, ThemePref};
use crate::db::{Db, settings};
use crate::download_mgr::{DownloadManager, FlashKind, RealBridge, UiEvent};
use crate::format_pref_from_str;
use crate::formats;
use crate::model::{UiQueueRow, split_pasted_urls};
use crate::ui_row_for_test;
use crate::url_open;
use crate::{DesignTokens, MainWindow, QueueRow, ToastEntry, VENDOR_PRIVACY_URL};

/// UC 11: monotonically-increasing id source for toast entries.
///
/// `Relaxed` ordering is correct here: each toast id is observed only by
/// the slint UI thread (which both pushes and dismisses), so there is no
/// cross-thread happens-before relationship that needs the stronger
/// `SeqCst` semantics. The only requirement is uniqueness, which `Relaxed`
/// provides via the atomic increment itself.
fn next_toast_id() -> i32 {
    use std::sync::atomic::{AtomicI32, Ordering};
    static COUNTER: AtomicI32 = AtomicI32::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// UC 11: hops to the slint event loop and pushes a toast onto the
/// `MainWindow.toasts` `VecModel`. Caps the visible queue at 3 by evicting
/// the oldest entry; each toast then auto-dismisses after the Slint-side
/// 3 s `Timer`. Works from any tokio task — callers do NOT need to be on
/// the UI thread first.
fn push_toast_on_main(weak: Weak<MainWindow>, kind: &'static str, text: &'static str) {
    let _ = slint::invoke_from_event_loop(move || {
        let Some(w) = weak.upgrade() else { return };
        let model = w.get_toasts();
        let Some(vec_model) = model.as_any().downcast_ref::<VecModel<ToastEntry>>() else {
            tracing::warn!("toasts model is not a VecModel; cannot push toast");
            return;
        };
        if vec_model.row_count() >= 3 {
            vec_model.remove(0);
        }
        vec_model.push(ToastEntry {
            id: next_toast_id(),
            text: SharedString::from(text),
            kind: SharedString::from(kind),
            visible_now: true,
        });
    });
}

/// Reads events from the manager and pushes them onto the Slint event loop.
///
/// Runs forever; the function returns only when `rx` is closed (i.e. all
/// senders dropped, which happens at app shutdown).
pub async fn run_ui_bridge(weak: Weak<MainWindow>, mut rx: mpsc::Receiver<UiEvent>) {
    while let Some(event) = rx.recv().await {
        let weak = weak.clone();
        let result = slint::invoke_from_event_loop(move || {
            apply_event(&weak, event);
        });
        if result.is_err() {
            tracing::warn!("Slint event loop closed; UI bridge exiting");
            break;
        }
    }
}

fn apply_event(weak: &Weak<MainWindow>, event: UiEvent) {
    let Some(window) = weak.upgrade() else {
        return;
    };
    match event {
        UiEvent::RowUpserted(row) => {
            upsert_row(&window, row);
            recompute_counts(&window);
        }
        UiEvent::RowRemoved(id) => {
            remove_row(&window, i32::try_from(id).unwrap_or(0));
            recompute_counts(&window);
        }
        UiEvent::Flash { message, kind } => {
            window.set_flash_message(SharedString::from(message));
            window.set_flash_kind(SharedString::from(flash_kind_str(kind)));
        }
        UiEvent::SettingsChanged => {
            // The UI is expected to re-read via the relevant getter on its
            // own callback path; nothing to do here today.
        }
        UiEvent::ShowBotCheckDialog { available } => {
            // Repopulate the popup model with the freshly detected list and
            // raise the open flag. UC 09: use `display_name()` so the popup
            // reads "Brave / Chrome / …" instead of yt-dlp's lowercase
            // arguments.
            let model = std::rc::Rc::new(VecModel::<SharedString>::from(
                available
                    .iter()
                    .map(|b| SharedString::from(b.display_name()))
                    .collect::<Vec<_>>(),
            ));
            window.set_bot_check_options(model.into());

            // UC 10: per-browser visibility flags drive the modal's row
            // rendering. Set every flag from the same detected list so a
            // host that detects only Chrome + Firefox lights up exactly
            // those two rows.
            window.set_bot_check_has_brave(available.contains(&Browser::Brave));
            window.set_bot_check_has_chrome(available.contains(&Browser::Chrome));
            window.set_bot_check_has_chromium(available.contains(&Browser::Chromium));
            window.set_bot_check_has_edge(available.contains(&Browser::Edge));
            window.set_bot_check_has_firefox(available.contains(&Browser::Firefox));
            window.set_bot_check_has_opera(available.contains(&Browser::Opera));
            window.set_bot_check_has_safari(available.contains(&Browser::Safari));
            window.set_bot_check_has_vivaldi(available.contains(&Browser::Vivaldi));

            // UC 10: compute the default-pick BEFORE setting `open = true`
            // so the modal's `states […]` block applies the correct value
            // when entering `open-state`. The session-default is the last
            // browser the user picked (held in `bot-check-last-pick`),
            // falling back to the first detected entry in canonical order.
            let last_pick = window.get_bot_check_last_pick();
            let last_pick_str = last_pick.as_str();
            let last_pick_arg = if last_pick_str.is_empty() {
                None
            } else {
                Browser::from_display_name(last_pick_str).map(|b| b.as_yt_dlp_arg())
            };
            let option_args: Vec<&str> = available.iter().map(Browser::as_yt_dlp_arg).collect();
            let default_pick =
                crate::bot_check::default_browser_for_open(last_pick_arg, &option_args)
                    .unwrap_or("");
            window.set_bot_check_default_pick(SharedString::from(default_pick));

            window.set_bot_check_open(true);
        }
        UiEvent::BotCheckAffectedCount { count } => {
            window.set_bot_check_affected_count(i32::try_from(count).unwrap_or(i32::MAX));
        }
        UiEvent::RowWaitingOnUser { id, waiting } => {
            set_row_waiting(&window, i32::try_from(id).unwrap_or(0), waiting);
        }
        UiEvent::ThumbnailReady { id, path } => {
            set_row_thumbnail(&window, i32::try_from(id).unwrap_or(0), &path);
        }
    }
}

/// Sets the thumbnail-related fields on a row (UC 08). Loads the on-disk
/// image, flips `thumbnail-loaded` so the gradient placeholder crossfades
/// out. Image-load failures degrade silently — the row keeps its placeholder.
fn set_row_thumbnail(window: &MainWindow, id: i32, path: &std::path::Path) {
    let model = window.get_queue();
    let Ok(image) = slint::Image::load_from_path(path) else {
        tracing::warn!(
            path = %path.display(),
            id,
            "thumbnail load failed; row keeps gradient placeholder"
        );
        return;
    };
    for i in 0..model.row_count() {
        if let Some(mut row) = model.row_data(i)
            && row.id == id
        {
            row.thumbnail_path = image;
            row.thumbnail_loaded = true;
            model.set_row_data(i, row);
            return;
        }
    }
}

/// UC 14: returns true when "Start all" is actionable — at least one row is
/// in a resumable state (`queued`, `cancelled`, or `error`) AND the previous
/// batch is not still in flight.
fn compute_start_all_enabled(queued: i32, cancelled: i32, error: i32, busy: bool) -> bool {
    !busy && (queued + cancelled + error) > 0
}

/// UC 14: builds the hover tooltip text for the Start-all button. When the
/// button is disabled (no resumable rows or a batch in flight) returns
/// `"Nothing to start"`. Otherwise returns a comma-joined breakdown that
/// omits each segment whose count is zero — e.g. `"3 queued, 2 error"` when
/// only queued and error rows exist. Pluralization is intentionally absent
/// (per UC 14 § AC 11).
fn compute_start_all_tooltip(queued: i32, cancelled: i32, error: i32, busy: bool) -> String {
    if !compute_start_all_enabled(queued, cancelled, error, busy) {
        return "Nothing to start".to_string();
    }
    let mut segments: Vec<String> = Vec::with_capacity(3);
    if queued > 0 {
        segments.push(format!("{queued} queued"));
    }
    if cancelled > 0 {
        segments.push(format!("{cancelled} cancelled"));
    }
    if error > 0 {
        segments.push(format!("{error} error"));
    }
    segments.join(", ")
}

/// Recomputes the footer counts (`active-count`, `queued-count`, `done-count`,
/// `waiting-count`) plus the UC 14 additions (`cancelled-count`,
/// `error-count`, `start-all-tooltip`) from the current Slint model. Called
/// on every `RowUpserted` / `RowRemoved` apply so the footer's mono strip
/// and the Start-all button's enable predicate + tooltip stay consistent
/// without per-state bookkeeping in Rust.
fn recompute_counts(window: &MainWindow) {
    let model = window.get_queue();
    let mut active = 0i32;
    let mut queued = 0i32;
    let mut done = 0i32;
    let mut waiting = 0i32;
    let mut cancelled = 0i32;
    let mut error = 0i32;
    for i in 0..model.row_count() {
        if let Some(row) = model.row_data(i) {
            if row.waiting_on_user {
                waiting += 1;
            }
            match row.status.as_str() {
                "in_flight" => active += 1,
                "queued" => queued += 1,
                "done" => done += 1,
                "cancelled" => cancelled += 1,
                "error" => error += 1,
                _ => {}
            }
        }
    }
    window.set_active_count(active);
    window.set_queued_count(queued);
    window.set_done_count(done);
    window.set_waiting_count(waiting);
    window.set_cancelled_count(cancelled);
    window.set_error_count(error);
    let busy = window.get_start_all_busy();
    let tooltip = compute_start_all_tooltip(queued, cancelled, error, busy);
    window.set_start_all_tooltip(SharedString::from(tooltip));
}

fn set_row_waiting(window: &MainWindow, id: i32, waiting: bool) {
    let model = window.get_queue();
    for i in 0..model.row_count() {
        if let Some(mut row) = model.row_data(i)
            && row.id == id
        {
            row.waiting_on_user = waiting;
            model.set_row_data(i, row);
            return;
        }
    }
}

fn upsert_row(window: &MainWindow, row: UiQueueRow) {
    let model = window.get_queue();
    let mut found = false;
    let mut idx = 0usize;
    let new_row = ui_row_for_test(row);
    for i in 0..model.row_count() {
        if let Some(existing) = model.row_data(i)
            && existing.id == new_row.id
        {
            idx = i;
            found = true;
            break;
        }
    }
    if found {
        model.set_row_data(idx, new_row);
    } else {
        // Append. Need to coerce ModelRc to a VecModel so we can push.
        if let Some(vec_model) = model.as_any().downcast_ref::<VecModel<QueueRow>>() {
            vec_model.push(new_row);
        } else {
            tracing::warn!("queue model is not a VecModel; cannot append");
        }
    }
}

fn remove_row(window: &MainWindow, id: i32) {
    let model = window.get_queue();
    if let Some(vec_model) = model.as_any().downcast_ref::<VecModel<QueueRow>>() {
        for i in 0..vec_model.row_count() {
            if let Some(row) = vec_model.row_data(i)
                && row.id == id
            {
                vec_model.remove(i);
                return;
            }
        }
    }
}

fn flash_kind_str(kind: FlashKind) -> &'static str {
    match kind {
        FlashKind::Info => "info",
        FlashKind::Duplicate => "duplicate",
        FlashKind::Error => "error",
    }
}

/// Wires every UI callback to the corresponding manager method.
#[allow(clippy::too_many_lines)]
pub fn wire_callbacks(
    window: &MainWindow,
    rt: &Handle,
    manager: &DownloadManager<RealBridge>,
    db: &Db,
) {
    {
        let manager = manager.clone();
        let rt = rt.clone();
        let weak = window.as_weak();
        window.on_add_urls(move |raw, audio_only| {
            let urls = split_pasted_urls(raw.as_str());
            if urls.is_empty() {
                return;
            }
            // UC 19: derive the per-URL FormatPref override from the
            // AddBar's "Audio only" toggle once. `Option<FormatPref>` is
            // `Copy`, so it threads into each `add_url` call without `.clone()`.
            let format_override = crate::format_pref_from_audio_only_flag(audio_only);
            let manager = manager.clone();
            let weak = weak.clone();
            rt.spawn(async move {
                let mut inserted_total = 0usize;
                let mut duplicate_count = 0usize;
                let mut error_count = 0usize;
                let mut bot_check_count = 0usize;
                for url in urls {
                    match manager.add_url(url.clone(), format_override).await {
                        Ok(crate::download_mgr::AddOutcome::Inserted { count }) => {
                            inserted_total += count;
                        }
                        Err(crate::download_mgr::AddError::DuplicateUrl(_)) => {
                            duplicate_count += 1;
                        }
                        Err(crate::download_mgr::AddError::Bridge(
                            yt_dlp_bridge::BridgeError::AuthRequired { .. },
                        )) => {
                            bot_check_count += 1;
                        }
                        Err(err) => {
                            tracing::warn!(?err, "add_url failed");
                            error_count += 1;
                        }
                    }
                }
                let flash = build_flash(
                    inserted_total,
                    duplicate_count,
                    error_count,
                    bot_check_count,
                );
                let weak_for_flash = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak_for_flash.upgrade() {
                        match flash {
                            Some((msg, kind)) => {
                                w.set_flash_message(SharedString::from(msg));
                                w.set_flash_kind(SharedString::from(kind));
                            }
                            None => {
                                // Caller still issues set_flash_message("") to
                                // clear stale strips on suppressed branches.
                                w.set_flash_message(SharedString::from(""));
                            }
                        }
                    }
                });
                // UC 11: add-failure toast — fired only when the dispatch
                // suppressed the flash strip (errors with no bot-check).
                if error_count > 0 && bot_check_count == 0 {
                    push_toast_on_main(weak.clone(), "danger", "Failed to add URL(s).");
                }
            });
        });
    }

    {
        // UC 14: Start all — broadens beyond `queued` to also resume
        // `cancelled` rows and retry `error` rows. The DB transaction +
        // emit_row fan-out is async, so the callback flips
        // `start-all-busy` synchronously (mid-flight gate, AC #7) and
        // recomputes the tooltip before spawning the async work, then
        // flips it back from inside the slint event loop on completion.
        let manager = manager.clone();
        let rt = rt.clone();
        let weak = window.as_weak();
        window.on_start_all(move || {
            if let Some(w) = weak.upgrade() {
                if w.get_start_all_busy() {
                    tracing::warn!("start_all clicked while busy; ignoring");
                    return;
                }
                w.set_start_all_busy(true);
                let queued = w.get_queued_count();
                let cancelled = w.get_cancelled_count();
                let error = w.get_error_count();
                let tooltip = compute_start_all_tooltip(queued, cancelled, error, true);
                w.set_start_all_tooltip(SharedString::from(tooltip));
            } else {
                return;
            }
            let manager = manager.clone();
            let weak_for_done = weak.clone();
            rt.spawn(async move {
                if let Err(err) = manager.start_all().await {
                    tracing::warn!(?err, "start_all failed");
                }
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak_for_done.upgrade() {
                        w.set_start_all_busy(false);
                        let queued = w.get_queued_count();
                        let cancelled = w.get_cancelled_count();
                        let error = w.get_error_count();
                        let tooltip = compute_start_all_tooltip(queued, cancelled, error, false);
                        w.set_start_all_tooltip(SharedString::from(tooltip));
                    }
                });
            });
        });
    }

    {
        let manager = manager.clone();
        let rt = rt.clone();
        window.on_cancel_clicked(move |id| {
            let manager = manager.clone();
            rt.spawn(async move {
                manager.cancel_one(i64::from(id)).await;
            });
        });
    }
    {
        let manager = manager.clone();
        let rt = rt.clone();
        window.on_remove_clicked(move |id| {
            let manager = manager.clone();
            rt.spawn(async move {
                if let Err(err) = manager.remove_one(i64::from(id)).await {
                    tracing::warn!(id, ?err, "remove_one failed");
                }
            });
        });
    }
    {
        let manager = manager.clone();
        let rt = rt.clone();
        window.on_restart_clicked(move |id| {
            let manager = manager.clone();
            rt.spawn(async move {
                if let Err(err) = manager.restart_one(i64::from(id)).await {
                    tracing::warn!(id, ?err, "restart_one failed");
                }
            });
        });
    }
    {
        // UC 11: empty-queue gate — fire the "Queue cancelled." info toast
        // only when there was actually something to cancel. Reading the
        // counts requires a hop back onto the slint event loop (Slint
        // properties are UI-thread only), so we round-trip via a tokio
        // oneshot to keep the read on the right thread.
        let manager = manager.clone();
        let rt = rt.clone();
        let weak = window.as_weak();
        window.on_cancel_all(move || {
            let manager = manager.clone();
            let weak = weak.clone();
            rt.spawn(async move {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let weak_for_read = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak_for_read.upgrade() {
                        let had_work =
                            w.get_queued_count() + w.get_active_count() + w.get_waiting_count() > 0;
                        let _ = tx.send(had_work);
                    }
                });
                let had_work = rx.await.unwrap_or(false);
                manager.cancel_all().await;
                if had_work {
                    push_toast_on_main(weak.clone(), "info", "Queue cancelled.");
                }
            });
        });
    }

    {
        let manager = manager.clone();
        window.on_start_one(move |id| {
            manager.start_one(i64::from(id));
        });
    }

    {
        let db = db.clone();
        let rt = rt.clone();
        let weak = window.as_weak();
        window.on_pick_destination_dir(move || {
            let db = db.clone();
            let weak = weak.clone();
            rt.spawn(async move {
                let dialog = rfd::AsyncFileDialog::new();
                let picked: Option<PathBuf> = dialog.pick_folder().await.map(|h| h.path().into());
                if let Some(dir) = picked {
                    let db_for_set = db.clone();
                    let dir_for_set = dir.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        db_for_set.with_conn(|c| settings::set_dest_dir(c, &dir_for_set))
                    })
                    .await;
                    let display = formats::format_dest_dir(&dir);
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak.upgrade() {
                            w.set_dest_dir_display(SharedString::from(display));
                        }
                    });
                }
            });
        });
    }

    {
        let db = db.clone();
        let rt = rt.clone();
        window.on_set_format_pref(move |s| {
            let db = db.clone();
            let pref = format_pref_from_str(s.as_str());
            rt.spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    db.with_conn(|c| settings::set_format_pref(c, pref))
                })
                .await;
            });
        });
    }

    {
        let db = db.clone();
        let rt = rt.clone();
        window.on_set_cookies_browser(move |s| {
            let db = db.clone();
            let choice = browser_from_display(s.as_str());
            rt.spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    db.with_conn(|c| settings::set_cookies_browser(c, choice))
                })
                .await;
            });
        });
    }

    {
        let db = db.clone();
        let rt = rt.clone();
        window.on_set_concurrency_cap(move |v| {
            let db = db.clone();
            let cap = u32::try_from(v).unwrap_or(3);
            rt.spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    db.with_conn(|c| settings::set_concurrency_cap(c, cap))
                })
                .await;
            });
        });
    }

    {
        let db = db.clone();
        let rt = rt.clone();
        window.on_set_focus_mode(move |v| {
            let db = db.clone();
            rt.spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    db.with_conn(|c| settings::set_focus_mode(c, v))
                })
                .await;
            });
        });
    }

    {
        let db = db.clone();
        let rt = rt.clone();
        window.on_set_ads_personalized(move |v| {
            let db = db.clone();
            rt.spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    db.with_conn(|c| settings::set_ads_personalized(c, v))
                })
                .await;
            });
        });
    }

    {
        window.on_open_vendor_privacy_policy(move || {
            let vendor_url = VENDOR_PRIVACY_URL;
            if vendor_url.is_empty() {
                tracing::warn!(
                    "open_vendor_privacy_policy fired with no vendor URL configured; ignoring"
                );
                return;
            }
            if let Err(err) = url_open::open(vendor_url) {
                tracing::warn!(?err, "url_open failed");
            }
        });
    }

    {
        // UC 18: ffmpeg LGPL § 4 source link in the About modal. Opens
        // https://ffmpeg.org/ in the system default browser. Failure is
        // logged at WARN — same posture as the vendor-privacy link.
        window.on_open_ffmpeg_source(move || {
            if let Err(err) = url_open::open("https://ffmpeg.org/") {
                tracing::warn!(?err, "url_open failed for ffmpeg source link");
            }
        });
    }

    {
        let manager = manager.clone();
        let db = db.clone();
        let rt = rt.clone();
        let weak = window.as_weak();
        window.on_bot_check_pick(move |s, remember| {
            let coordinator = manager.bot_check_coordinator();
            let db = db.clone();
            let Some(browser) = browser_from_display(s.as_str()) else {
                return;
            };
            // UC 10: persist the in-memory session default so subsequent
            // bot-check opens within the same session pre-select this
            // pick, regardless of whether `remember` was checked. The
            // Slint mount also writes this synchronously on click; the
            // Rust write here is defense in depth and uses the canonical
            // yt-dlp arg form so it matches the modal's id-keyed state.
            if let Some(w) = weak.upgrade() {
                w.set_bot_check_last_pick(SharedString::from(browser.as_yt_dlp_arg()));
            }
            rt.spawn(async move {
                if let Err(err) = coordinator.user_picked(browser, remember, &db).await {
                    tracing::warn!(?err, "bot_check user_picked failed");
                }
            });
        });
    }

    {
        let manager = manager.clone();
        let rt = rt.clone();
        window.on_bot_check_cancel(move || {
            let coordinator = manager.bot_check_coordinator();
            rt.spawn(async move {
                let _ = coordinator.user_cancelled().await;
            });
        });
    }

    {
        let db = db.clone();
        let rt = rt.clone();
        let weak = window.as_weak();
        window.on_toggle_theme(move || {
            // The Slint side has already flipped `DesignTokens.dark-mode`
            // before invoking this callback (see the moon/sun button in
            // main_window.slint). Read the post-flip value back and persist.
            let Some(w) = weak.upgrade() else { return };
            let tokens = w.global::<DesignTokens<'_>>();
            let explicit = if tokens.get_dark_mode() {
                ExplicitTheme::Dark
            } else {
                ExplicitTheme::Light
            };
            let db = db.clone();
            rt.spawn(async move {
                let res = tokio::task::spawn_blocking(move || {
                    db.with_conn(|c| settings::set_theme(c, explicit))
                })
                .await;
                if let Ok(Err(err)) = res {
                    tracing::warn!(
                        ?err,
                        "failed to persist theme; toggle still applied for the session"
                    );
                }
            });
        });
    }

    {
        // UC 11: id-based lookup so that a stale Toast firing its timer
        // after a sibling has been evicted (which shifts indices) cannot
        // remove the wrong row.
        let weak = window.as_weak();
        window.on_dismiss_toast(move |id| {
            let Some(w) = weak.upgrade() else { return };
            let model = w.get_toasts();
            let Some(vec_model) = model.as_any().downcast_ref::<VecModel<ToastEntry>>() else {
                return;
            };
            for i in 0..vec_model.row_count() {
                if let Some(row) = vec_model.row_data(i)
                    && row.id == id
                {
                    vec_model.remove(i);
                    return;
                }
            }
        });
    }
}

/// Reads the persisted theme preference at startup and seeds
/// `DesignTokens.dark-mode`. DB failures degrade to light mode for the
/// session.
///
/// `ThemePref::System` resolves to light here; Slint 1.16.1 does not
/// expose the OS color scheme on the public Rust `Window` API
/// (`color_scheme()` lives on the internal `WindowInner`, not on the
/// public `slint::Window`). Resolving system mode properly requires
/// reading `Palette.color-scheme` from inside `tokens.slint` and piping
/// it back via a property — deferred. The use case explicitly allows
/// `light` as the fallback; once the user toggles, the explicit choice
/// persists and exits system mode permanently per AC#6.
pub fn seed_theme_from_settings(window: &MainWindow, db: &Db) {
    let pref = match db.with_conn(settings::get_theme) {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(
                ?err,
                "failed to read theme; defaulting to light for this session"
            );
            ThemePref::Light
        }
    };
    let dark = match pref {
        ThemePref::Light | ThemePref::System => false,
        ThemePref::Dark => true,
    };
    window.global::<DesignTokens<'_>>().set_dark_mode(dark);
}

/// Resolves a display string from the cookies dropdown / bot-check popup
/// back to a [`Browser`] variant. Delegates to
/// [`Browser::from_display_name`] so case-insensitive lookups also succeed
/// against legacy lowercase yt-dlp argument tokens — keeping the rename
/// from `as_yt_dlp_arg`-keyed UI strings to display names in UC 09 a
/// non-breaking change.
fn browser_from_display(s: &str) -> Option<Browser> {
    Browser::from_display_name(s)
}

fn build_flash(
    inserted: usize,
    duplicates: usize,
    errors: usize,
    bot_checks: usize,
) -> Option<(String, &'static str)> {
    // bot_check supersedes generic error in dispatch.
    if bot_checks > 0 {
        return None;
    }
    if errors > 0 {
        return None;
    }
    if duplicates > 0 && inserted == 0 {
        return Some(("Already in queue".to_string(), "duplicate"));
    }
    if duplicates > 0 {
        return Some((
            format!("Added {inserted} item(s); {duplicates} duplicate(s) skipped"),
            "duplicate",
        ));
    }
    if inserted > 0 {
        return Some((format!("Added {inserted} item(s)"), "info"));
    }
    None
}

#[cfg(test)]
mod tests {
    //! UC 14: pure-helper tests for the `Start all` enable predicate and
    //! tooltip text. These don't construct a `MainWindow` — they exercise
    //! `compute_start_all_enabled` and `compute_start_all_tooltip` directly,
    //! which is the whole reason those helpers are pulled out of
    //! `recompute_counts` / `on_start_all`.
    use super::{compute_start_all_enabled, compute_start_all_tooltip};

    // ----- compute_start_all_enabled -------------------------------------

    #[test]
    fn enabled_false_when_all_zero_and_idle() {
        assert!(!compute_start_all_enabled(0, 0, 0, false));
    }

    #[test]
    fn enabled_false_when_all_zero_and_busy() {
        assert!(!compute_start_all_enabled(0, 0, 0, true));
    }

    #[test]
    fn enabled_false_when_busy_even_with_resumable_rows() {
        // (1, 0, 0, true) and other busy variants — busy gate dominates.
        assert!(!compute_start_all_enabled(1, 0, 0, true));
        assert!(!compute_start_all_enabled(0, 1, 0, true));
        assert!(!compute_start_all_enabled(0, 0, 1, true));
        assert!(!compute_start_all_enabled(3, 2, 5, true));
    }

    #[test]
    fn enabled_true_for_each_single_segment_when_idle() {
        assert!(compute_start_all_enabled(1, 0, 0, false));
        assert!(compute_start_all_enabled(0, 1, 0, false));
        assert!(compute_start_all_enabled(0, 0, 1, false));
    }

    #[test]
    fn enabled_true_for_paired_segments_when_idle() {
        assert!(compute_start_all_enabled(0, 2, 3, false));
        assert!(compute_start_all_enabled(3, 0, 5, false));
        assert!(compute_start_all_enabled(3, 2, 0, false));
    }

    #[test]
    fn enabled_true_when_all_segments_present_and_idle() {
        assert!(compute_start_all_enabled(1, 1, 1, false));
        assert!(compute_start_all_enabled(3, 2, 5, false));
    }

    #[test]
    fn enabled_full_segment_omission_matrix_idle() {
        // 8 combinations of (queued, cancelled, error) ∈ {0, nonzero}^3 with
        // busy=false. Only (0,0,0) is disabled; every other combination is
        // enabled. AC #2 (enable widens to queued+cancelled+error > 0).
        for &q in &[0i32, 3] {
            for &c in &[0i32, 2] {
                for &e in &[0i32, 5] {
                    let expected = (q + c + e) > 0;
                    assert_eq!(
                        compute_start_all_enabled(q, c, e, false),
                        expected,
                        "idle (q={q}, c={c}, e={e}) expected enabled={expected}"
                    );
                }
            }
        }
    }

    #[test]
    fn enabled_full_segment_omission_matrix_busy() {
        // Same 8 combinations with busy=true: every result is false.
        // AC #7 (busy gate covers the bulk-SQL window).
        for &q in &[0i32, 3] {
            for &c in &[0i32, 2] {
                for &e in &[0i32, 5] {
                    assert!(
                        !compute_start_all_enabled(q, c, e, true),
                        "busy (q={q}, c={c}, e={e}) must always disable"
                    );
                }
            }
        }
    }

    // ----- compute_start_all_tooltip -------------------------------------

    #[test]
    fn tooltip_all_zero_idle_reads_nothing_to_start() {
        // (0,0,0, false) — disabled-state tooltip.
        assert_eq!(
            compute_start_all_tooltip(0, 0, 0, false),
            "Nothing to start"
        );
    }

    #[test]
    fn tooltip_all_zero_busy_reads_nothing_to_start() {
        assert_eq!(compute_start_all_tooltip(0, 0, 0, true), "Nothing to start");
    }

    #[test]
    fn tooltip_busy_with_resumable_rows_still_reads_nothing_to_start() {
        // (*, *, *, true) — the busy gate trumps the breakdown text so the
        // user does not see a "<N> queued" promise while the SQL bulk-reset
        // is mid-flight.
        assert_eq!(compute_start_all_tooltip(3, 2, 5, true), "Nothing to start");
        assert_eq!(compute_start_all_tooltip(1, 0, 0, true), "Nothing to start");
        assert_eq!(compute_start_all_tooltip(0, 1, 0, true), "Nothing to start");
        assert_eq!(compute_start_all_tooltip(0, 0, 1, true), "Nothing to start");
    }

    #[test]
    fn tooltip_only_queued_segment_idle() {
        assert_eq!(compute_start_all_tooltip(3, 0, 0, false), "3 queued");
    }

    #[test]
    fn tooltip_only_cancelled_segment_idle() {
        assert_eq!(compute_start_all_tooltip(0, 2, 0, false), "2 cancelled");
    }

    #[test]
    fn tooltip_only_error_segment_idle() {
        assert_eq!(compute_start_all_tooltip(0, 0, 5, false), "5 error");
    }

    #[test]
    fn tooltip_queued_and_cancelled_segments_idle() {
        assert_eq!(
            compute_start_all_tooltip(3, 2, 0, false),
            "3 queued, 2 cancelled"
        );
    }

    #[test]
    fn tooltip_queued_and_error_segments_idle() {
        assert_eq!(
            compute_start_all_tooltip(3, 0, 5, false),
            "3 queued, 5 error"
        );
    }

    #[test]
    fn tooltip_cancelled_and_error_segments_idle() {
        assert_eq!(
            compute_start_all_tooltip(0, 2, 5, false),
            "2 cancelled, 5 error"
        );
    }

    #[test]
    fn tooltip_all_three_segments_idle() {
        // AC #11 — full breakdown ordering: queued, cancelled, error.
        assert_eq!(
            compute_start_all_tooltip(3, 2, 5, false),
            "3 queued, 2 cancelled, 5 error"
        );
    }

    #[test]
    fn tooltip_full_segment_omission_matrix_idle() {
        // Sweep all 8 combinations. (0,0,0) → "Nothing to start"; every
        // other combination is the comma-joined breakdown with each
        // zero-valued segment dropped. Pinned to lock the exact wording
        // contract from AC #11.
        struct Case {
            q: i32,
            c: i32,
            e: i32,
            expected: &'static str,
        }
        let cases = [
            Case {
                q: 0,
                c: 0,
                e: 0,
                expected: "Nothing to start",
            },
            Case {
                q: 3,
                c: 0,
                e: 0,
                expected: "3 queued",
            },
            Case {
                q: 0,
                c: 2,
                e: 0,
                expected: "2 cancelled",
            },
            Case {
                q: 0,
                c: 0,
                e: 5,
                expected: "5 error",
            },
            Case {
                q: 3,
                c: 2,
                e: 0,
                expected: "3 queued, 2 cancelled",
            },
            Case {
                q: 3,
                c: 0,
                e: 5,
                expected: "3 queued, 5 error",
            },
            Case {
                q: 0,
                c: 2,
                e: 5,
                expected: "2 cancelled, 5 error",
            },
            Case {
                q: 3,
                c: 2,
                e: 5,
                expected: "3 queued, 2 cancelled, 5 error",
            },
        ];
        for case in &cases {
            assert_eq!(
                compute_start_all_tooltip(case.q, case.c, case.e, false),
                case.expected,
                "idle (q={}, c={}, e={})",
                case.q,
                case.c,
                case.e,
            );
        }
    }
}
