# Application Design

_Started 2026-04-30 05:27 UTC_

---

## User

I want to create a design for an application I'm building. Target platform is PC (MacOS, Windows, Linux). 
This is the context provided by claude code about the application:

## Assistant

I'll design a high-fidelity desktop app for yt-dlp-ui. Let me ask a few focused questions before building, since this is a brand-new design with no existing UI kit to reference.

_[tool: questions_v2]_

## User

Questions answered:
- theme: Both (toggle)
- aesthetic: Clean utilitarian (think native macOS Finder / Transmission)
- accent: Purple/violet
- ad_slot_placement: Bottom strip (full width, slim)
- thumbnails: Yes — small thumbnail per row
- queue_density: Spacious (large thumbnails, big progress)
- platform_chrome: All three side-by-side
- variations: Light vs dark theme, Settings panel open/closed
- states_to_show: All of the above
- interactivity: Lightly interactive (hover, settings open, modal open)
- anything_else: It should be something I can later provide to claude code for implementation

## Assistant

I've got everything I need. Building a clean utilitarian desktop app design with light/dark theme, purple/violet accent, spacious rows with thumbnails, bottom-strip ad slot, all three OS frames side-by-side, and lightly interactive (settings + modal toggles).

**Design system declaration:** Inter for UI, JetBrains Mono for monospace/URLs/numbers. Cool neutral grays. Single accent oklch(0.58 0.18 295) (violet). Native-feeling controls — no rounded-corner-with-left-border tropes. Will commit to this throughout.

_[tool: update_todos]_

_[tool: snip]_

_[tool: copy_starter_component]_

_[tool: copy_starter_component]_

_[tool: copy_starter_component]_

I'll build OS chrome inline (Windows 11 + GNOME aren't in starters, and macOS native styling for an app like this is custom enough that I'll write all three for consistency). Let me start writing the design.

_[tool: read_file]_

Got it. Now let me build the design. I'll structure it as multiple JSX files to keep things manageable.

_[tool: write_file]_

Now the icons + thumbnails helpers, then the queue row, then the main app.

_[tool: write_file]_

_[tool: write_file]_

Now the queue row component:

_[tool: write_file]_

Now the OS window chrome (all three) and the main app shell:

_[tool: write_file]_

_[tool: snip]_

_[tool: write_file]_

_[tool: snip]_

_[tool: write_file]_

_[tool: snip]_

_[tool: write_file]_

_[tool: snip]_

_[tool: write_file]_

_[tool: done]_

_[tool: snip]_

_[tool: fork_verifier_agent]_

Designed yt-dlp-ui as a clean utilitarian Slint desktop app with a violet accent.

**What's on the canvas:**
- **Design notes card** — design system + implementation notes for handoff to Claude Code
- **Main window × 3 OSes** (macOS, Windows 11, Linux/GNOME) in light theme, all showing the same mid-download scenario covering every row state: in-flight w/ progress, queued, fetching metadata, cancelled, waiting-on-user, error, done
- **Same trio in dark theme**
- **Settings panel** open (light + dark) — three tabs: General (format, destination, concurrency), Cookies (browser source), Privacy & Ads (Focus mode, consent)
- **Bot-check modal** open (light + dark) — browser picker w/ "remember this choice"
- **Empty states** in both themes

**Lightly interactive (within each artboard):**
- Moon/sun icon → flips theme on the fly
- Gear icon → opens Settings
- Cancel-all → triggers the "Queue cancelled" toast
- Per-row Cancel/Restart/Remove all work
- In-flight progress bars animate live

