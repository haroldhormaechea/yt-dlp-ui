/* global React */
// OS window chrome — macOS, Windows 11, Linux (GNOME). Not pixel-perfect
// recreations; original takes inspired by the platforms' general patterns.

const OSFrame = ({ os = 'macos', title = 'yt-dlp-ui', theme = 'light', children, width = 920, height = 640 }) => {
  return (
    <div
      style={{
        width,
        height,
        borderRadius: os === 'macos' ? 11 : os === 'windows' ? 7 : 12,
        overflow: 'hidden',
        background: theme === 'dark' ? '#161618' : '#f6f6f7',
        boxShadow:
          '0 22px 60px rgba(0,0,0,0.18), 0 4px 12px rgba(0,0,0,0.08), 0 0 0 1px rgba(0,0,0,0.06)',
        display: 'flex',
        flexDirection: 'column',
        position: 'relative',
      }}
    >
      <TitleBar os={os} title={title} theme={theme} />
      <div style={{ flex: 1, minHeight: 0, position: 'relative' }}>{children}</div>
    </div>
  );
};

const TitleBar = ({ os, title, theme }) => {
  if (os === 'macos') return <MacBar title={title} theme={theme} />;
  if (os === 'windows') return <WinBar title={title} theme={theme} />;
  return <LinuxBar title={title} theme={theme} />;
};

const MacBar = ({ title, theme }) => {
  const dark = theme === 'dark';
  return (
    <div
      style={{
        height: 28,
        flexShrink: 0,
        background: dark ? '#252529' : '#e9e9ed',
        borderBottom: `1px solid ${dark ? '#0f0f12' : '#d3d3d8'}`,
        display: 'flex',
        alignItems: 'center',
        padding: '0 12px',
        position: 'relative',
        fontFamily: '-apple-system, BlinkMacSystemFont, sans-serif',
      }}
    >
      <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
        <div style={{ width: 12, height: 12, borderRadius: '50%', background: '#ff5f57', boxShadow: 'inset 0 0 0 0.5px rgba(0,0,0,0.15)' }} />
        <div style={{ width: 12, height: 12, borderRadius: '50%', background: '#febc2e', boxShadow: 'inset 0 0 0 0.5px rgba(0,0,0,0.15)' }} />
        <div style={{ width: 12, height: 12, borderRadius: '50%', background: '#28c840', boxShadow: 'inset 0 0 0 0.5px rgba(0,0,0,0.15)' }} />
      </div>
      <div
        style={{
          position: 'absolute',
          left: 0, right: 0,
          textAlign: 'center',
          fontSize: 12.5,
          fontWeight: 600,
          color: dark ? '#d6d6da' : '#3a3a3f',
          pointerEvents: 'none',
          letterSpacing: 0.01,
        }}
      >
        {title}
      </div>
    </div>
  );
};

const WinBar = ({ title, theme }) => {
  const dark = theme === 'dark';
  return (
    <div
      style={{
        height: 32,
        flexShrink: 0,
        background: dark ? '#1d1d20' : '#f6f6f7',
        borderBottom: `1px solid ${dark ? '#2a2a2f' : '#e2e2e6'}`,
        display: 'flex',
        alignItems: 'center',
        fontFamily: '"Segoe UI Variable Display", "Segoe UI", sans-serif',
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '0 12px', flex: 1 }}>
        <div
          style={{
            width: 14, height: 14, borderRadius: 3,
            background: 'linear-gradient(135deg, oklch(0.58 0.18 295), oklch(0.7 0.16 295))',
          }}
        />
        <div style={{ fontSize: 12, color: dark ? '#d4d4d8' : '#3a3a3f', fontWeight: 400 }}>
          {title}
        </div>
      </div>
      <div style={{ display: 'flex', height: '100%' }}>
        <WinControl glyph="min" dark={dark} />
        <WinControl glyph="max" dark={dark} />
        <WinControl glyph="close" dark={dark} close />
      </div>
    </div>
  );
};
const WinControl = ({ glyph, dark, close }) => (
  <div
    style={{
      width: 46, height: '100%',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      color: dark ? '#d4d4d8' : '#3a3a3f',
    }}
  >
    <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1">
      {glyph === 'min' && <path d="M1.5 5h7" />}
      {glyph === 'max' && <rect x="1" y="1" width="8" height="8" />}
      {glyph === 'close' && <path d="M1 1l8 8M9 1l-8 8" />}
    </svg>
  </div>
);

const LinuxBar = ({ title, theme }) => {
  const dark = theme === 'dark';
  return (
    <div
      style={{
        height: 38,
        flexShrink: 0,
        background: dark ? '#242428' : '#f6f5f4',
        borderBottom: `1px solid ${dark ? '#1a1a1d' : '#dad8d4'}`,
        display: 'flex',
        alignItems: 'center',
        fontFamily: '"Cantarell", "Inter", sans-serif',
        padding: '0 8px 0 12px',
        gap: 8,
      }}
    >
      <div style={{ flex: 1, fontSize: 13, fontWeight: 700, color: dark ? '#e3e3e6' : '#2e2e30' }}>
        {title}
      </div>
      <div style={{ display: 'flex', gap: 6 }}>
        <LinuxBtn dark={dark}>
          <svg width="8" height="8" viewBox="0 0 8 8" fill="none" stroke="currentColor" strokeWidth="1.2"><path d="M1 4h6" /></svg>
        </LinuxBtn>
        <LinuxBtn dark={dark}>
          <svg width="8" height="8" viewBox="0 0 8 8" fill="none" stroke="currentColor" strokeWidth="1.2"><rect x="1" y="1" width="6" height="6" /></svg>
        </LinuxBtn>
        <LinuxBtn dark={dark}>
          <svg width="8" height="8" viewBox="0 0 8 8" fill="none" stroke="currentColor" strokeWidth="1.2"><path d="M1.5 1.5l5 5M6.5 1.5l-5 5" /></svg>
        </LinuxBtn>
      </div>
    </div>
  );
};
const LinuxBtn = ({ dark, children }) => (
  <div
    style={{
      width: 24, height: 24, borderRadius: '50%',
      background: dark ? '#3a3a3e' : '#e1dfdb',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      color: dark ? '#d4d4d8' : '#4a4a50',
    }}
  >
    {children}
  </div>
);

window.OSFrame = OSFrame;
