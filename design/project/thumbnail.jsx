/* global React, Icon */
// Thumbnail placeholder — colored gradient block with mono caption.
// We don't draw fake video stills; we use abstract striped tiles labelled
// with the source name so it's clearly a placeholder.

const THUMB_PALETTES = [
  ['#3b3b66', '#5e4480'], // violet
  ['#1f4d52', '#2e7079'], // teal
  ['#5b3a36', '#7d4a44'], // rust
  ['#2d3a4f', '#3f536d'], // slate-blue
  ['#4d3a52', '#6b4670'], // mauve
  ['#3f4426', '#5e6433'], // olive
  ['#503020', '#73493a'], // burnt
  ['#1a3b3a', '#266b5d'], // forest
];

const hashStr = (s) => {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) | 0;
  return Math.abs(h);
};

const Thumbnail = ({ source = "youtube", seed = "x", duration, label, width = 96, height = 54 }) => {
  const palette = THUMB_PALETTES[hashStr(seed) % THUMB_PALETTES.length];
  const stripeShift = (hashStr(seed + "s") % 30) - 15;
  return (
    <div
      style={{
        width,
        height,
        borderRadius: 4,
        position: 'relative',
        overflow: 'hidden',
        flexShrink: 0,
        background: `linear-gradient(135deg, ${palette[0]} 0%, ${palette[1]} 100%)`,
        boxShadow: 'inset 0 0 0 1px rgba(255,255,255,0.04)',
      }}
    >
      {/* Diagonal stripe pattern - signals "placeholder" */}
      <div
        style={{
          position: 'absolute',
          inset: 0,
          backgroundImage: `repeating-linear-gradient(${115 + stripeShift}deg, rgba(255,255,255,0.045) 0 6px, transparent 6px 14px)`,
        }}
      />
      {/* Source label */}
      <div
        style={{
          position: 'absolute',
          top: 4, left: 5,
          fontFamily: 'var(--font-mono)',
          fontSize: 9,
          color: 'rgba(255,255,255,0.55)',
          letterSpacing: 0.02,
          textTransform: 'lowercase',
        }}
      >
        {source}
      </div>
      {/* Duration tag */}
      {duration && (
        <div
          style={{
            position: 'absolute',
            bottom: 4, right: 4,
            fontFamily: 'var(--font-mono)',
            fontSize: 9.5,
            color: 'white',
            background: 'rgba(0,0,0,0.55)',
            padding: '1px 4px',
            borderRadius: 2,
            letterSpacing: 0.02,
          }}
        >
          {duration}
        </div>
      )}
      {/* Tiny play glyph faintly centered */}
      <div
        style={{
          position: 'absolute',
          inset: 0,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: 'rgba(255,255,255,0.32)',
        }}
      >
        <svg width="18" height="18" viewBox="0 0 16 16">
          <path d="M5 3.5v9l7-4.5z" fill="currentColor" />
        </svg>
      </div>
    </div>
  );
};

window.Thumbnail = Thumbnail;
