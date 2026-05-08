# 0003 — Monetization model

- **Status:** accepted
- **Date:** 2026-04-25

## Context

The project has commercial intent. Local-only desktop scope rules out
SaaS / subscription / usage-metered / marketplace / advertising-of-our-own-
inventory models. Plausible candidates: donations, third-party ads,
one-time purchase, freemium, sponsorware.

## Decision

**Donations + in-app advertising via a third-party ad-network SDK.** Both
revenue streams run in parallel. No paid tiers, no one-time purchase, no
SaaS, no subscriptions.

- Donations are user-discretion via GitHub Sponsors / Ko-fi /
  OpenCollective links surfaced in the UI.
- Advertising uses a third-party ad-network SDK (vendor TBD). Telemetry
  collection by the SDK is permitted in exchange for the higher revenue
  ceiling that JS-based ad networks deliver.
- A **first-launch consent flow** with plain-language disclosure is a hard
  MVP requirement (PROJECT_BRIEF.md § Monetization). The UI must surface
  the SDK's name, the fact that it collects device/behavioral data, a link
  to the vendor's privacy policy, and a GDPR/CCPA-appropriate consent
  affordance.

## Consequences

**Positive:**
- Two revenue streams hedged against each other. Donation income is small
  but non-zero for OSS-spirited projects; ad income scales with usage.
- Telemetry tradeoff is **explicit** to the user, not hidden. Aligns with
  stated "respect upstream's spirit" constraint.

**Negative:**
- "No telemetry" is not achievable with a third-party ad SDK. The project
  itself collects no first-party telemetry, but whatever the SDK does, the
  user inherits. This is documented and consented to, not concealed.
- Adds a network dependency to the otherwise local-only app (the ad
  WebView fetches creative). Users running offline will see no ads, but
  the app must continue to function fully without them.
- Ad-vendor selection is deferred. The chosen vendor will materially
  affect the privacy story, the SDK footprint, and possibly the ad-window
  IPC requirements. Re-evaluate the consent flow and threat model when a
  vendor is picked.

## Alternatives considered

- **OSS, no monetization** — pure free download, donations only. Lower
  revenue ceiling.
- **OSS + donations** — same. Insufficient on its own per stated intent.
- **One-time purchase, closed source** — would commercialize work derived
  from upstream yt-dlp's community labor. Reputational risk, even though
  legally permitted by upstream's Unlicense.
- **Ads with no telemetry (static / sponsor-style)** — achievable but
  modest revenue ceiling. Listed for completeness; not chosen.

## References

- PROJECT_BRIEF.md § Monetization
- THREATS.md § T3 (untrusted third-party ad-SDK code)
