#!/usr/bin/env node
// One-shot converter from the OKLCH literals in design/project/tokens.css
// to sRGB hex strings suitable for paste into crates/app/ui/design/tokens.slint.
//
// Usage:
//   npm install culori@4 --no-save
//   node tools/oklch-to-srgb.js
//
// Pinned dependency: culori 4.x (latest 4.x at time of writing: 4.0.1).
// Output is a plain text table; values are clipped to the sRGB gamut, matching
// what a browser would render for the same OKLCH literal.
//
// This script is run by a human, not by the Cargo build. The committed
// tokens.slint file holds the already-converted hex values; this script exists
// so the conversion is reproducible and reviewable.

const { oklch, formatHex, clampChroma } = require('culori');

const TOKENS = [
  // [name, L, C, H, theme]
  ['accent',          0.58, 0.18, 295, 'light'],
  ['accent-hover',    0.52, 0.19, 295, 'light'],
  ['accent-soft',     0.96, 0.03, 295, 'light'],
  ['accent-border',   0.85, 0.08, 295, 'light'],
  ['accent-text',     0.42, 0.18, 295, 'light'],
  ['success',         0.62, 0.14, 155, 'light'],
  ['success-soft',    0.95, 0.04, 155, 'light'],
  ['success-text',    0.42, 0.13, 155, 'light'],
  ['warning',         0.72, 0.15,  75, 'light'],
  ['warning-soft',    0.96, 0.05,  80, 'light'],
  ['warning-text',    0.48, 0.14,  70, 'light'],
  ['danger',          0.58, 0.18,  25, 'light'],
  ['danger-soft',     0.96, 0.03,  25, 'light'],
  ['danger-text',     0.45, 0.16,  25, 'light'],
  ['muted',           0.65, 0.01, 270, 'light'],
  ['muted-soft',      0.95, 0.005,270, 'light'],

  ['accent',          0.72, 0.16, 295, 'dark'],
  ['accent-hover',    0.78, 0.17, 295, 'dark'],
  ['accent-soft',     0.32, 0.09, 295, 'dark'],
  ['accent-border',   0.42, 0.12, 295, 'dark'],
  ['accent-text',     0.82, 0.13, 295, 'dark'],
  ['success',         0.72, 0.14, 155, 'dark'],
  ['success-soft',    0.30, 0.06, 155, 'dark'],
  ['success-text',    0.80, 0.12, 155, 'dark'],
  ['warning',         0.78, 0.15,  75, 'dark'],
  ['warning-soft',    0.32, 0.07,  75, 'dark'],
  ['warning-text',    0.85, 0.13,  80, 'dark'],
  ['danger',          0.70, 0.18,  25, 'dark'],
  ['danger-soft',     0.32, 0.08,  25, 'dark'],
  ['danger-text',     0.82, 0.14,  25, 'dark'],
  ['muted',           0.62, 0.01, 270, 'dark'],
  ['muted-soft',      0.28, 0.005,270, 'dark'],
];

console.log('# OKLCH -> sRGB hex (clipped to gamut). Theme-grouped.\n');
let theme = null;
for (const [name, l, c, h, t] of TOKENS) {
  if (t !== theme) {
    console.log(`\n## ${t}`);
    theme = t;
  }
  const color = { mode: 'oklch', l, c, h };
  const clipped = clampChroma(color, 'oklch', 'rgb');
  const hex = formatHex(clipped);
  console.log(`${name.padEnd(16)}  oklch(${l} ${c} ${h})  ->  ${hex}`);
}
