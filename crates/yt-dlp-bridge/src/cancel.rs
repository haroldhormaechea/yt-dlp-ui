//! Two-stage subprocess termination shared by the download supervisor and
//! the cancellable metadata fetch path.
//!
//! On Unix: send `SIGTERM` to the child via [`nix::sys::signal::kill`], race
//! a 2-second grace timer against `child.wait()`. If the timer wins, fall
//! through to `child.start_kill()` (which sends `SIGKILL`) and reap the
//! process.
//!
//! On Windows: `TerminateProcess` is the only termination primitive, and it
//! is synchronous and immediate. There is no kernel-level analog of `SIGTERM`
//! that yt-dlp could intercept to clean up. The Windows branch therefore
//! collapses to `child.start_kill()` + `child.wait()` with no grace period.
//! See `PROJECT_BRIEF.md` § Architecture § Cancellation for the cross-platform
//! contract.

use std::time::Duration;

/// Grace window between `SIGTERM` and `SIGKILL` on Unix.
const GRACE: Duration = Duration::from_secs(2);

/// Terminate `child` cooperatively when possible, forcefully otherwise.
///
/// Unix:
/// 1. `kill(pid, SIGTERM)`. `ESRCH` is treated as success — the child has
///    already exited and there is nothing left to signal.
/// 2. `tokio::select!` races `tokio::time::sleep(GRACE)` against
///    `child.wait()`. On wait win, the child exited cleanly; we are done.
/// 3. On grace-timer win, `child.start_kill()` issues `SIGKILL` and we
///    `child.wait().await` so the OS resources are reaped.
///
/// Windows:
/// `child.start_kill()` + `child.wait().await`. No `SIGTERM` analog.
pub(crate) async fn terminate_with_grace(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    {
        let Some(pid_raw) = child.id() else {
            tracing::debug!("cancel: child has no pid (already reaped); nothing to signal");
            return;
        };
        let pid = nix::unistd::Pid::from_raw(pid_raw.cast_signed());
        match nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM) {
            Ok(()) => tracing::debug!(pid = pid_raw, "cancel: SIGTERM sent"),
            Err(nix::errno::Errno::ESRCH) => {
                tracing::debug!(pid = pid_raw, "cancel: child already gone (ESRCH)");
                return;
            }
            Err(err) => {
                tracing::debug!(
                    pid = pid_raw,
                    ?err,
                    "cancel: SIGTERM failed; escalating to SIGKILL"
                );
                let _ = child.start_kill();
                let _ = child.wait().await;
                return;
            }
        }

        tokio::select! {
            res = child.wait() => {
                tracing::debug!(?res, pid = pid_raw, "cancel: child exited within grace period");
            }
            () = tokio::time::sleep(GRACE) => {
                tracing::debug!(pid = pid_raw, "cancel: grace expired; sending SIGKILL");
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
        }
    }

    #[cfg(not(unix))]
    {
        tracing::debug!("cancel: Windows path — start_kill (immediate TerminateProcess)");
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}
