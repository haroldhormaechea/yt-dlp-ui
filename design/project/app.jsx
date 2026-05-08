/* global React, Icon, QueueRow, SettingsPanel, BotCheckModal */
// Main app shell — add bar, queue, footer, ad slot, toasts.

const SAMPLE_DATA = {
  empty: [],
  mixed: [
    {
      id: '1',
      title: 'How to refactor a 50,000-line codebase without crying',
      url: 'https://www.youtube.com/watch?v=aB3kP2',
      source: 'youtube',
      status: 'in_flight',
      pct: 64.2,
      speed: '8.4 MB/s',
      eta: '0:42',
      size: '184 MB',
      downloaded: '118 MB',
      duration: '24:18',
      seed: 'refactor-talk',
    },
    {
      id: '2',
      title: 'Eberhard Weber Quintet — Live at North Sea Jazz 2007',
      url: 'https://www.youtube.com/watch?v=Lk9Hp2',
      source: 'youtube',
      status: 'queued',
      pct: 0,
      duration: '1:04:12',
      seed: 'jazz-set',
    },
    {
      id: '3',
      title: 'GDC 2024: Procedural Animation in Indie Games',
      url: 'https://www.youtube.com/watch?v=Mk2nR1',
      source: 'youtube',
      status: 'queued',
      pct: 0,
      duration: '47:33',
      seed: 'gdc-anim',
    },
    {
      id: '4',
      title: 'Casserole de poireaux au gruyère — recette express',
      url: 'https://vimeo.com/892341078',
      source: 'vimeo',
      status: 'done',
      pct: 100,
      size: '92 MB',
      duration: '7:54',
      seed: 'cooking',
    },
    {
      id: '5',
      title: '',
      url: 'https://www.youtube.com/watch?v=p8sLpQ_VfTw',
      source: 'youtube',
      status: 'queued',
      fetching: true,
      pct: 0,
      seed: 'fetching',
    },
    {
      id: '6',
      title: 'Patrick Boucheron — Histoire mondiale de la France',
      url: 'https://www.youtube.com/watch?v=hG3pK1',
      source: 'youtube',
      status: 'cancelled',
      pct: 31.8,
      size: '218 MB',
      downloaded: '69 MB',
      duration: '1:32:44',
      seed: 'lecture',
    },
    {
      id: '7',
      title: 'Cosmos Laundromat — Open Movie',
      url: 'https://www.youtube.com/watch?v=Y-rmzhQAcCM',
      source: 'youtube',
      status: 'waiting_on_user',
      pct: 0,
      duration: '12:14',
      seed: 'cosmos',
    },
    {
      id: '8',
      title: 'Late-night drum solo (private)',
      url: 'https://www.youtube.com/watch?v=zZ9qF2',
      source: 'youtube',
      status: 'error',
      pct: 0,
      duration: '4:21',
      seed: 'drums',
      error: 'Video unavailable: this video is private. Sign in if you have access.',
    },
  ],
};

const App = ({ initialTheme = 'light', initialSettingsOpen = false, initialModalOpen = false, scenario = 'mixed' }) => {
  const [theme, setTheme] = React.useState(initialTheme);
  const [settingsOpen, setSettingsOpen] = React.useState(initialSettingsOpen);
  const [modalOpen, setModalOpen] = React.useState(initialModalOpen);
  const [items, setItems] = React.useState(SAMPLE_DATA[scenario] || SAMPLE_DATA.mixed);
  const [hoverId, setHoverId] = React.useState(null);
  const [addText, setAddText] = React.useState('');
  const [showDenoBanner, setShowDenoBanner] = React.useState(scenario === 'mixed');
  const [showQueueCancelToast, setShowQueueCancelToast] = React.useState(false);

  const [settings, setSettings] = React.useState({
    format: 'Best video (bestvideo+bestaudio/best)',
    concurrency: 3,
    cookies: 'None',
    focus: false,
    adsConsent: true,
  });

  // Animate the in-flight progress for liveness
  React.useEffect(() => {
    const t = setInterval(() => {
      setItems((prev) =>
        prev.map((i) => {
          if (i.status !== 'in_flight') return i;
          const next = Math.min(99.4, i.pct + 0.4 + Math.random() * 0.5);
          return { ...i, pct: next };
        })
      );
    }, 700);
    return () => clearInterval(t);
  }, []);

  const handleAction = (id, action) => {
    if (action === 'remove') {
      setItems((prev) => prev.filter((i) => i.id !== id));
    } else if (action === 'cancel') {
      setItems((prev) =>
        prev.map((i) =>
          i.id === id ? { ...i, status: 'cancelled' } : i
        )
      );
    } else if (action === 'restart') {
      setItems((prev) =>
        prev.map((i) => (i.id === id ? { ...i, status: 'queued', pct: 0 } : i))
      );
    } else if (action === 'start') {
      setItems((prev) =>
        prev.map((i) => (i.id === id ? { ...i, status: 'in_flight' } : i))
      );
    }
  };

  const cancelAll = () => {
    setItems((prev) =>
      prev.map((i) =>
        i.status === 'queued' || i.status === 'in_flight' || i.status === 'waiting_on_user'
          ? { ...i, status: 'cancelled' }
          : i
      )
    );
    setShowQueueCancelToast(true);
    setTimeout(() => setShowQueueCancelToast(false), 3000);
  };

  const startAll = () => {
    setItems((prev) =>
      prev.map((i, idx) =>
        i.status === 'queued' && idx < settings.concurrency
          ? { ...i, status: 'in_flight' }
          : i
      )
    );
  };

  const queuedCount = items.filter((i) => i.status === 'queued' || i.status === 'in_flight').length;
  const activeCount = items.filter((i) => i.status === 'in_flight').length;

  return (
    <div
      className="app"
      data-theme={theme}
      style={{
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        position: 'relative',
        overflow: 'hidden',
      }}
    >
      {/* Add bar */}
      <div
        style={{
          padding: '12px 14px',
          background: 'var(--surface)',
          borderBottom: '1px solid var(--border)',
          display: 'flex',
          gap: 8,
          flexShrink: 0,
        }}
      >
        <div style={{ position: 'relative', flex: 1 }}>
          <Icon
            name="link"
            size={13}
            style={{
              position: 'absolute', left: 10, top: '50%',
              transform: 'translateY(-50%)',
              color: 'var(--text-3)',
            }}
          />
          <input
            className="input"
            style={{ width: '100%', paddingLeft: 30 }}
            placeholder="Paste a video or playlist URL — multiple lines supported"
            value={addText}
            onChange={(e) => setAddText(e.target.value)}
          />
        </div>
        <button className="btn btn-primary" style={{ height: 30 }}>
          <Icon name="plus" size={13} /> Add
        </button>
        <button
          className="btn btn-icon"
          style={{ height: 30, width: 30 }}
          onClick={() => setTheme(theme === 'light' ? 'dark' : 'light')}
          title={`Switch to ${theme === 'light' ? 'dark' : 'light'} theme`}
        >
          <Icon name={theme === 'light' ? 'moon' : 'sun'} size={13} />
        </button>
        <button
          className="btn btn-icon"
          style={{ height: 30, width: 30 }}
          onClick={() => setSettingsOpen(true)}
          title="Settings"
        >
          <Icon name="gear" size={13} />
        </button>
      </div>

      {/* Deno banner */}
      {showDenoBanner && (
        <div
          style={{
            padding: '7px 14px',
            background: 'var(--warning-soft)',
            borderBottom: '1px solid var(--border)',
            color: 'var(--warning-text)',
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            fontSize: 11.5,
            flexShrink: 0,
          }}
        >
          <Icon name="info" size={13} />
          <div style={{ flex: 1 }}>
            Some YouTube downloads may require Deno.{' '}
            <span className="mono" style={{ background: 'rgba(0,0,0,0.06)', padding: '1px 5px', borderRadius: 3 }}>
              brew install deno
            </span>{' '}
            (or platform equivalent).
          </div>
          <button
            className="btn btn-icon btn-sm btn-ghost"
            onClick={() => setShowDenoBanner(false)}
          >
            <Icon name="close" size={11} />
          </button>
        </div>
      )}

      {/* Queue list */}
      <div
        className="scroll"
        style={{
          flex: 1,
          minHeight: 0,
          overflowY: 'auto',
          background: 'var(--surface)',
        }}
      >
        {items.length === 0 ? (
          <EmptyState />
        ) : (
          items.map((item) => (
            <div
              key={item.id}
              onMouseEnter={() => setHoverId(item.id)}
              onMouseLeave={() => setHoverId(null)}
            >
              <QueueRow item={item} hovered={hoverId === item.id} onAction={handleAction} />
            </div>
          ))
        )}
      </div>

      {/* Footer (batch actions) */}
      <div
        style={{
          padding: '10px 14px',
          background: 'var(--surface-2)',
          borderTop: '1px solid var(--border)',
          display: 'flex',
          alignItems: 'center',
          gap: 10,
          flexShrink: 0,
        }}
      >
        <div style={{ display: 'flex', gap: 8 }}>
          <button
            className="btn"
            onClick={startAll}
            disabled={items.filter((i) => i.status === 'queued').length === 0}
          >
            <Icon name="play" size={11} /> Start all queued
          </button>
          <button
            className="btn btn-danger"
            onClick={cancelAll}
            disabled={queuedCount === 0}
          >
            <Icon name="stop" size={11} /> Cancel all
          </button>
        </div>
        <div style={{ flex: 1 }} />
        <div className="mono" style={{ fontSize: 11, color: 'var(--text-3)', display: 'flex', gap: 14 }}>
          <span>{activeCount} active</span>
          <span>{items.filter((i) => i.status === 'queued').length} queued</span>
          <span>{items.filter((i) => i.status === 'done').length} done</span>
          <span style={{ color: 'var(--text-3)' }}>cap {settings.concurrency}</span>
        </div>
      </div>

      {/* Ad slot - bottom strip */}
      {!settings.focus && (
        <div
          style={{
            height: 64,
            flexShrink: 0,
            background: theme === 'dark' ? '#0d0d0f' : '#eaeaee',
            borderTop: '1px solid var(--border)',
            display: 'flex',
            alignItems: 'center',
            padding: '0 14px',
            gap: 10,
            position: 'relative',
          }}
        >
          <div
            style={{
              fontSize: 9,
              fontFamily: 'var(--font-mono)',
              color: 'var(--text-3)',
              textTransform: 'uppercase',
              letterSpacing: 0.06,
              writingMode: 'vertical-rl',
              transform: 'rotate(180deg)',
              padding: '4px 0',
            }}
          >
            Ad
          </div>
          <div
            style={{
              flex: 1,
              height: 48,
              borderRadius: 4,
              background: theme === 'dark'
                ? 'repeating-linear-gradient(115deg, #1a1a1d 0 8px, #18181b 8px 18px)'
                : 'repeating-linear-gradient(115deg, #d8d8de 0 8px, #d2d2d8 8px 18px)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              color: 'var(--text-3)',
              fontFamily: 'var(--font-mono)',
              fontSize: 11,
              letterSpacing: 0.04,
              border: '1px dashed var(--border-strong)',
            }}
          >
            ad slot · WebView render area · 728×48
          </div>
          <button
            className="btn btn-sm btn-ghost"
            onClick={() => setSettings({ ...settings, focus: true })}
            title="Hide ad (Focus mode)"
          >
            <Icon name="eye" size={12} /> Focus
          </button>
        </div>
      )}

      {/* Toast */}
      {showQueueCancelToast && <Toast text="Queue cancelled." />}

      {/* Settings */}
      <SettingsPanel
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        settings={settings}
        setSettings={setSettings}
      />

      {/* Modal */}
      <BotCheckModal
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        affectedCount={items.filter((i) => i.status === 'waiting_on_user').length || 1}
      />
    </div>
  );
};

const EmptyState = () => (
  <div
    style={{
      height: '100%',
      minHeight: 280,
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'center',
      justifyContent: 'center',
      gap: 10,
      padding: 24,
      color: 'var(--text-3)',
    }}
  >
    <div
      style={{
        width: 56, height: 56, borderRadius: 12,
        background: 'var(--surface-3)',
        border: '1px solid var(--border)',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        color: 'var(--text-2)',
      }}
    >
      <Icon name="download" size={22} stroke={1.6} />
    </div>
    <div style={{ fontSize: 13.5, fontWeight: 500, color: 'var(--text-2)', marginTop: 6 }}>
      Queue is empty
    </div>
    <div style={{ fontSize: 12, lineHeight: 1.55, textAlign: 'center', maxWidth: 320 }}>
      Paste a video or playlist URL above to get started. Multi-line paste
      adds them all at once.
    </div>
  </div>
);

const Toast = ({ text }) => (
  <div
    style={{
      position: 'absolute',
      bottom: 80,
      left: '50%',
      transform: 'translateX(-50%)',
      background: 'var(--text)',
      color: 'var(--bg)',
      padding: '8px 14px',
      borderRadius: 6,
      fontSize: 12,
      fontWeight: 500,
      boxShadow: 'var(--shadow-md)',
      zIndex: 8,
      animation: 'toastIn 200ms cubic-bezier(.2,.8,.3,1)',
    }}
  >
    {text}
    <style>{`@keyframes toastIn { from { opacity: 0; transform: translate(-50%, 8px); } }
    @keyframes qrShimmer { from { background-position: 0 0; } to { background-position: 17px 0; } }`}</style>
  </div>
);

window.App = App;
