# Use Case 30: Clean up UC 27 CI failures

## Summary

The UC 27 merge (commit `6da3636`) landed via `--admin` override with `cargo clippy` and `cargo test` red on Ubuntu / macOS / Windows. None indicates a functional regression — the 234-test main suite passes — but the lints obscure future signal and the failing test misleads anyone running the suite. Two workstreams: (a) fix ~21 clippy lints in the `app` crate that surfaced once UC 27's larger code volume entered the workspace (`clippy::similar_names`, `clippy::manual_let_else`, `clippy::collapsible_if`, `clippy::doc_markdown`, `clippy::large_enum_variant`, `clippy::cast_possible_wrap`, `clippy::cast_sign_loss`, `clippy::nonminimal_bool`, `clippy::map_unwrap_or`, `clippy::explicit_counter_loop`, `clippy::manual_saturating_arithmetic`, `clippy::single_match_else`); (b) update the stale test `add_url_fetches_and_caches_thumbnail` in `crates/app/tests/thumbnail_pipeline.rs:200` whose 3-second `ThumbnailReady` deadline doesn't cover UC 27's new placeholder → `enumerate_playlist_cancellable` → `promote_placeholder_to_video` → `fetch_metadata_cancellable` → `spawn_thumbnail_fetch` chain. Scope is bounded by the existing failure list — no broader audit, no scope creep into unrelated lint policy.

## Acceptance Criteria

1. `cargo clippy --workspace --all-targets -- -D warnings` exits 0 on master CI (Ubuntu runner).
2. `cargo test --workspace` passes on Ubuntu, macOS, and Windows CI runners.
3. The thumbnail-pipeline test `add_url_fetches_and_caches_thumbnail` has its `ThumbnailReady` deadline raised to ~10 seconds to cover the new flow's worst-case latency on the slowest CI runner. The test logic stays unchanged (assert `ThumbnailReady` emitted + cached file on disk under `<cache_dir>` with the SHA-1-hex + `.jpg` filename shape).
4. No `#[allow(clippy::...)]` blanket suppressions are added to silence the surveyed lints; each lint must be resolved by code change. Narrow-scope suppressions (single function or single expression) are allowed only for genuine false positives, each carrying a one-line `// reason: …` rationale.
5. `clippy::large_enum_variant` on `EnumerationOutcome` is resolved by **boxing the `Playlist` variant** — `EnumerationOutcome::Playlist(Box<Vec<PlaylistEntry>>)`. Every callsite across the workspace must be updated to the boxed shape. The bridge crate's public API changes accordingly; the change is mentioned explicitly in the PR description.
6. No production-code behavior change ships in this PR beyond the bridge crate's boxed-variant signature. Lint fixes that would require behavior change (e.g., altering saturating-arithmetic semantics, removing a method flagged by `dead_code` that has out-of-tree callers) must preserve existing behavior or be deferred out of scope with a note in the PR description.
7. **Master CI lands all-green** on the squash-merge commit. `--admin` merge is not permitted; the implementer must keep iterating until every required check is genuinely green before merge.

## Potential Pitfalls & Open Questions

- **Assumption** — the dead-code lint on `read_cookies_arg` (bridge `metadata.rs`) is safely removable. If a future UC plans to use it, deletion creates a regression risk. Grep the workspace before deleting; if no callsite exists, remove it.
- **Missing input** — the full enumerated list of `clippy::doc_markdown` hits is not in the failure summary (the lint log only shows the first hit per crate-target before clippy stops). The implementer pulls the complete clippy output once at the start of the work and addresses every hit in the same pass.
- **Edge case** — raising the `ThumbnailReady` deadline to ~10 s slows the happy path but does not change what's tested. If the test still flakes after the raise, the cause is likely an actual UC 27 wiring miss (e.g., `spawn_thumbnail_fetch` not invoked when `fetch_metadata` returns a thumbnail URL) — at that point the fix is in production code, not the test deadline, and the implementer escalates rather than raising the deadline further.

## Original Description

> ## UC 27 follow-ups (CI currently red on master)
>
> The PR merged with these still failing — none indicate functional bugs:
> - **cargo clippy** — 21+ lints (mix of `similar_names`, `let_else`, `single_match`, `nonminimal_bool`, `collapsible_if`, `large_enum_variant`, `manual_saturating`, dead `read_cookies_arg` method, plus a few `doc_markdown` hits). All stylistic.
> - **cargo test (all three OSes)** — one stale test `add_url_fetches_and_caches_thumbnail` in `thumbnail_pipeline.rs:200` whose `ThumbnailReady` deadline doesn't account for the new placeholder→enumerate→fetch_metadata→spawn_thumbnail_fetch flow taking >3 s in CI under load.
>
> Master CI will stay red until someone runs `cargo fmt --all`, `cargo clippy --workspace --all-targets --fix`, and updates that one thumbnail test's deadline (or its mock to bypass the metadata-fetch hop).

## Clarifications

- Q: Should AC #7 (master CI green without `--admin`) be a hard gate for closing UC 30?
  A: Hard gate — no `--admin` merge allowed.
- Q: If `clippy::large_enum_variant` fires on `EnumerationOutcome`, how should the implementer resolve it?
  A: Box the `Playlist` variant.
- Q: How should the failing `add_url_fetches_and_caches_thumbnail` test be fixed?
  A: Raise the `ThumbnailReady` deadline to ~10 s.
