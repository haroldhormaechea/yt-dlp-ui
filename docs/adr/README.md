# Architecture Decision Records

This directory holds the project's ADRs in MADR-style Markdown — one
decision per file, numbered sequentially with a short slug.

## Why ADRs

ADRs capture **why** a decision was made, not just **what** was decided. The
"why" is what's hardest to recover six months later when someone (often you)
asks "why on earth did we do it like that?". Each ADR is short — five to
fifteen minutes to write — and proportionally valuable.

## Convention

- File name: `NNNN-<slug>.md`, e.g. `0007-update-policy.md`. Sequential,
  zero-padded to four digits.
- Status: `proposed` / `accepted` / `superseded` / `deprecated`. Once an ADR
  is superseded, mark it `superseded by <NNNN>` and link the new ADR.
- Sections: **Context**, **Decision**, **Consequences**, **Alternatives**.
  Optionally **References**.

## Index

| # | Title | Status |
|---|---|---|
| 0001 | [Language and UI framework](0001-language-and-ui-framework.md) | accepted |
| 0002 | [License](0002-license.md) | accepted |
| 0003 | [Monetization model](0003-monetization-model.md) | accepted |
| 0004 | [Ad-window process isolation](0004-ad-window-process-isolation.md) | accepted |
| 0005 | [yt-dlp bundling](0005-yt-dlp-bundling.md) | accepted |
| 0006 | [Storage](0006-storage.md) | accepted |
| 0011 | [macOS signing and notarization](0011-macos-signing-and-notarization.md) | accepted |
