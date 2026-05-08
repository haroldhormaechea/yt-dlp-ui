# 0002 — License

- **Status:** accepted
- **Date:** 2026-04-25

## Context

The project must be licensable under terms that:

1. Allow forking, redistribution, modification, and personal/educational use.
2. Forbid third parties from commercializing the work and redistributing it.
3. Are compatible with the upstream yt-dlp Unlicense (which imposes nothing
   from above — any license is permitted on derivative work).

No OSI-approved open-source license satisfies (1) **and** (2)
simultaneously: OSI's definition explicitly forbids restrictions on fields
of use, including commerce. The honest framing is therefore
**source-available, non-commercial**, not "open source."

## Decision

Adopt **PolyForm Noncommercial License 1.0.0** for all UI and installer
code authored in this repository (everything under `crates/`, `examples/`,
`docs/`, the workspace manifests, `Justfile`, `deny.toml`, `THREATS.md`,
`CONTRIBUTING.md`, `LICENSE`, and any future `README.md`).

The upstream yt-dlp source tree retains its original **Unlicense** terms
via the existing `LICENSE` file at the repository root. That file is
**never overwritten**. The new PolyForm license lives in a separate file
(`LICENSE`) to make the dual-licensing reality explicit.

## Consequences

**Positive:**
- Clear, modern, lawyer-drafted license text. Plain English.
- Forks and modifications are permitted; commercial use is not. Matches
  stated intent.
- Distinct file (`LICENSE`) preserves upstream's `LICENSE` file
  untouched, respecting the read-only-upstream-tree rule.

**Negative:**
- The project is **not** "open source" by OSI's definition. Marketing must
  use "source-available, non-commercial," not "open source."
- Distro repositories that require OSI/FSF-approved licenses (Debian main,
  Fedora, Homebrew core) will refuse to ship the project. Distribution is
  via our own GitHub Releases, AppImage, snap, and `.deb`/`.rpm` directly.
- Some contributors avoid non-OSS licenses on principle; expect a smaller
  contributor pool than a permissive OSS project.
- Ad-related interaction with the license: running ads in the licensor's
  **own** distribution is permitted (the NC clause binds licensees, not
  the licensor). Forks may strip ads (allowed). Forks may NOT add their
  own ads or paid features and redistribute (forbidden by NC).

## Alternatives considered

- **CC BY-NC 4.0** — Creative Commons explicitly discourages CC for
  software (no patent grant, ambiguous on derivative-work mechanics for
  code). Rejected.
- **BUSL-1.1** — non-commercial for a fixed period (typically 4 years),
  then auto-converts to a permissive license. Heavier and more
  controversial than PolyForm; not warranted for this project's size.
- **MIT / Apache-2.0** — permissive OSS. Cannot block commercialization,
  conflicting with stated intent.
- **GPL-3.0 / AGPL-3.0** — copyleft OSS. Forces forks to stay open, but
  does NOT block commercialization of the original. Rejected.
- **Custom license** — legally fragile, scares off contributors, hard to
  enforce. Rejected.

## References

- https://polyformproject.org/licenses/noncommercial/1.0.0/
- PROJECT_BRIEF.md § Monetization
