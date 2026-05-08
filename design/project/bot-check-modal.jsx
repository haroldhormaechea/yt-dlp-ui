/* global React, Icon */
// Bot-check modal — choose a browser to pull cookies from.

const BROWSERS = [
  { id: 'brave', name: 'Brave', color: '#fb542b' },
  { id: 'chrome', name: 'Chrome', color: '#4285f4' },
  { id: 'firefox', name: 'Firefox', color: '#ff7139' },
  { id: 'safari', name: 'Safari', color: '#1f8df8' },
  { id: 'edge', name: 'Edge', color: '#0078d4' },
];

const BrowserGlyph = ({ id, color }) => (
  <div
    style={{
      width: 28, height: 28, borderRadius: '50%',
      background: `radial-gradient(circle at 30% 30%, ${color}dd 0%, ${color}88 70%, ${color}55 100%)`,
      boxShadow: 'inset 0 0 0 1px rgba(255,255,255,0.18), inset 0 -2px 4px rgba(0,0,0,0.15)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      flexShrink: 0,
    }}
  >
    <div style={{ width: 9, height: 9, borderRadius: '50%', background: 'rgba(255,255,255,0.85)' }} />
  </div>
);

const BotCheckModal = ({ open, onClose, affectedCount = 1 }) => {
  const [picked, setPicked] = React.useState('chrome');
  const [remember, setRemember] = React.useState(false);

  if (!open) return null;
  return (
    <>
      <div
        onClick={onClose}
        style={{
          position: 'absolute', inset: 0,
          background: 'rgba(10,10,15,0.42)',
          zIndex: 6,
          backdropFilter: 'blur(1px)',
        }}
      />
      <div
        role="dialog"
        aria-modal="true"
        style={{
          position: 'absolute',
          left: '50%', top: '50%',
          transform: 'translate(-50%, -50%)',
          width: 440,
          background: 'var(--surface)',
          borderRadius: 10,
          boxShadow: 'var(--shadow-lg)',
          border: '1px solid var(--border)',
          zIndex: 7,
          overflow: 'hidden',
          animation: 'modalIn 180ms cubic-bezier(.2,.8,.3,1)',
        }}
      >
        <div style={{ padding: '18px 20px 14px' }}>
          <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12 }}>
            <div
              style={{
                width: 32, height: 32,
                borderRadius: '50%',
                background: 'var(--warning-soft)',
                color: 'var(--warning-text)',
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                flexShrink: 0,
              }}
            >
              <Icon name="shield" size={16} />
            </div>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 14.5, fontWeight: 600, marginBottom: 4 }}>
                YouTube needs cookies to verify you're not a bot.
              </div>
              <div style={{ fontSize: 12, color: 'var(--text-2)', lineHeight: 1.5 }}>
                Pick a browser you're signed into YouTube with. yt-dlp-ui will
                read its cookies (locally, just once) and retry the download.
                {affectedCount > 1 && (
                  <>
                    {' '}This applies to{' '}
                    <strong style={{ color: 'var(--text)' }}>
                      {affectedCount} queued items
                    </strong>.
                  </>
                )}
              </div>
            </div>
          </div>
        </div>

        <div
          style={{
            margin: '0 20px',
            border: '1px solid var(--border)',
            borderRadius: 8,
            background: 'var(--surface-2)',
            overflow: 'hidden',
          }}
        >
          {BROWSERS.map((b, i) => (
            <button
              key={b.id}
              onClick={() => setPicked(b.id)}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 11,
                width: '100%',
                padding: '9px 12px',
                background: picked === b.id ? 'var(--accent-soft)' : 'transparent',
                border: 'none',
                borderTop: i === 0 ? 'none' : '1px solid var(--divider)',
                cursor: 'pointer',
                color: 'inherit',
                fontFamily: 'inherit',
                textAlign: 'left',
              }}
            >
              <BrowserGlyph id={b.id} color={b.color} />
              <div style={{ flex: 1, fontSize: 13, fontWeight: 500 }}>{b.name}</div>
              <div
                style={{
                  width: 16, height: 16, borderRadius: '50%',
                  border: `${picked === b.id ? 5 : 1.5}px solid ${
                    picked === b.id ? 'var(--accent)' : 'var(--border-strong)'
                  }`,
                  background: 'var(--surface)',
                  transition: 'border-width 100ms',
                }}
              />
            </button>
          ))}
        </div>

        <div
          style={{
            padding: '14px 20px',
            display: 'flex',
            alignItems: 'center',
            gap: 12,
          }}
        >
          <label style={{ display: 'flex', alignItems: 'center', gap: 7, cursor: 'pointer', fontSize: 12, color: 'var(--text-2)' }}>
            <input
              type="checkbox"
              checked={remember}
              onChange={(e) => setRemember(e.target.checked)}
              style={{ accentColor: 'oklch(0.58 0.18 295)', margin: 0 }}
            />
            Remember this choice
          </label>
          <div style={{ flex: 1 }} />
          <button className="btn" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary" onClick={onClose}>
            Use {BROWSERS.find((b) => b.id === picked)?.name}
          </button>
        </div>
      </div>
      <style>{`@keyframes modalIn { from { opacity: 0; transform: translate(-50%, -46%);} to { opacity: 1; transform: translate(-50%, -50%);} }`}</style>
    </>
  );
};

window.BotCheckModal = BotCheckModal;
