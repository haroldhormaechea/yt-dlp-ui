/* global React, Icon */
// Settings panel — slides in from the right.

const SettingsPanel = ({ open, onClose, settings, setSettings }) => {
  const [browserDetected] = React.useState(true);
  const tabs = ['General', 'Cookies', 'Privacy & Ads'];
  const [tab, setTab] = React.useState('General');

  const update = (k, v) => setSettings({ ...settings, [k]: v });

  return (
    <>
      {open && (
        <div
          onClick={onClose}
          style={{
            position: 'absolute', inset: 0,
            background: 'rgba(0,0,0,0.18)',
            zIndex: 4,
            opacity: open ? 1 : 0,
            transition: 'opacity 160ms',
          }}
        />
      )}
      <div
        style={{
          position: 'absolute',
          top: 0, right: 0, bottom: 0,
          width: 380,
          background: 'var(--surface)',
          borderLeft: '1px solid var(--border)',
          boxShadow: 'var(--shadow-lg)',
          transform: open ? 'translateX(0)' : 'translateX(100%)',
          transition: 'transform 200ms cubic-bezier(.2,.8,.3,1)',
          zIndex: 5,
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        {/* Header */}
        <div
          style={{
            padding: '14px 16px 0',
            borderBottom: '1px solid var(--divider)',
            flexShrink: 0,
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12 }}>
            <div style={{ fontSize: 14.5, fontWeight: 600 }}>Settings</div>
            <button className="btn btn-icon btn-sm btn-ghost" onClick={onClose} title="Close">
              <Icon name="close" size={12} />
            </button>
          </div>
          <div style={{ display: 'flex', gap: 2 }}>
            {tabs.map((t) => (
              <button
                key={t}
                onClick={() => setTab(t)}
                style={{
                  background: 'transparent',
                  border: 'none',
                  borderBottom: `2px solid ${tab === t ? 'var(--accent)' : 'transparent'}`,
                  color: tab === t ? 'var(--text)' : 'var(--text-2)',
                  fontFamily: 'inherit',
                  fontSize: 12.5,
                  fontWeight: tab === t ? 600 : 500,
                  padding: '8px 10px',
                  marginBottom: -1,
                  cursor: 'pointer',
                }}
              >
                {t}
              </button>
            ))}
          </div>
        </div>

        {/* Body */}
        <div className="scroll" style={{ flex: 1, overflowY: 'auto', padding: '18px 16px' }}>
          {tab === 'General' && (
            <>
              <Field label="Format preference" hint="Applies to subsequent downloads.">
                <select
                  className="select"
                  style={{ width: '100%' }}
                  value={settings.format}
                  onChange={(e) => update('format', e.target.value)}
                >
                  <option>Best video (bestvideo+bestaudio/best)</option>
                  <option>Best audio · MP3</option>
                  <option>Best audio · Opus</option>
                </select>
              </Field>

              <Field label="Download destination">
                <div style={{ display: 'flex', gap: 6 }}>
                  <div
                    className="input mono"
                    style={{
                      flex: 1, fontSize: 11.5, display: 'flex', alignItems: 'center',
                      color: 'var(--text-2)', overflow: 'hidden', textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                  >
                    ~/Downloads/yt-dlp-ui/
                  </div>
                  <button className="btn">
                    <Icon name="folder" size={13} /> Choose…
                  </button>
                </div>
              </Field>

              <Field label="Concurrency cap" hint={`Up to ${settings.concurrency} parallel downloads.`}>
                <Stepper
                  value={settings.concurrency}
                  min={1} max={10}
                  onChange={(v) => update('concurrency', v)}
                />
              </Field>
            </>
          )}

          {tab === 'Cookies' && (
            <>
              <Field
                label="Cookies source"
                hint={
                  browserDetected
                    ? 'Used only when YouTube asks for verification.'
                    : 'No supported browsers detected on this machine.'
                }
              >
                <select
                  className="select"
                  style={{ width: '100%' }}
                  value={settings.cookies}
                  onChange={(e) => update('cookies', e.target.value)}
                  disabled={!browserDetected}
                >
                  <option>None</option>
                  <option>Brave</option>
                  <option>Chrome</option>
                  <option>Chromium</option>
                  <option>Edge</option>
                  <option>Firefox</option>
                  <option>Opera</option>
                  <option>Safari</option>
                  <option>Vivaldi</option>
                </select>
              </Field>

              <InfoBox icon="shield">
                Cookies stay on this machine. yt-dlp-ui never uploads them anywhere.
              </InfoBox>
            </>
          )}

          {tab === 'Privacy & Ads' && (
            <>
              <Field label="Focus mode">
                <Toggle
                  on={settings.focus}
                  onChange={(v) => update('focus', v)}
                  label={settings.focus ? 'Ad slot hidden' : 'Ad slot visible'}
                />
              </Field>

              <Field label="Ad personalization">
                <Toggle
                  on={settings.adsConsent}
                  onChange={(v) => update('adsConsent', v)}
                  label={settings.adsConsent ? 'Personalized ads on' : 'Generic ads only'}
                />
              </Field>

              <div
                style={{
                  fontSize: 11.5,
                  color: 'var(--text-2)',
                  lineHeight: 1.55,
                  background: 'var(--surface-2)',
                  border: '1px solid var(--border)',
                  borderRadius: 6,
                  padding: 12,
                  marginTop: 4,
                }}
              >
                <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--text)', marginBottom: 6 }}>
                  How ads work here
                </div>
                yt-dlp-ui shows ads to keep development sustainable. The ad SDK
                collects device data (OS, locale, screen size) and behavioral
                signals to pick what to show.{' '}
                <a href="#" style={{ color: 'var(--accent-text)', textDecoration: 'underline' }}>
                  Read the vendor's privacy policy.
                </a>{' '}
                You can turn off personalization above, or hide the slot
                entirely with Focus mode.
              </div>
            </>
          )}
        </div>
      </div>
    </>
  );
};

const Field = ({ label, hint, children }) => (
  <div style={{ marginBottom: 18 }}>
    <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 6, color: 'var(--text)' }}>
      {label}
    </div>
    {children}
    {hint && (
      <div style={{ fontSize: 11.5, color: 'var(--text-3)', marginTop: 5, lineHeight: 1.45 }}>
        {hint}
      </div>
    )}
  </div>
);

const Stepper = ({ value, min, max, onChange }) => {
  const dec = () => onChange(Math.max(min, value - 1));
  const inc = () => onChange(Math.min(max, value + 1));
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
      <button className="btn btn-sm btn-icon" onClick={dec} disabled={value === min}>−</button>
      <div
        style={{
          flex: 1,
          height: 4, borderRadius: 2,
          background: 'var(--surface-3)',
          position: 'relative',
        }}
      >
        <div
          style={{
            position: 'absolute', left: 0, top: 0, bottom: 0,
            width: `${((value - min) / (max - min)) * 100}%`,
            background: 'var(--accent)', borderRadius: 2,
          }}
        />
        <div
          style={{
            position: 'absolute',
            left: `calc(${((value - min) / (max - min)) * 100}% - 7px)`,
            top: -5,
            width: 14, height: 14,
            background: 'var(--surface)',
            border: '1px solid var(--border-strong)',
            borderRadius: '50%',
            boxShadow: 'var(--shadow-sm)',
          }}
        />
      </div>
      <button className="btn btn-sm btn-icon" onClick={inc} disabled={value === max}>+</button>
      <div className="mono" style={{ width: 24, textAlign: 'right', fontSize: 12, color: 'var(--text)' }}>{value}</div>
    </div>
  );
};

const Toggle = ({ on, onChange, label }) => (
  <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
    <button
      onClick={() => onChange(!on)}
      style={{
        width: 32, height: 18,
        borderRadius: 9,
        background: on ? 'var(--accent)' : 'var(--border-strong)',
        border: 'none',
        position: 'relative',
        cursor: 'pointer',
        transition: 'background 120ms',
        padding: 0,
      }}
    >
      <div
        style={{
          width: 14, height: 14, borderRadius: '50%',
          background: 'white',
          position: 'absolute', top: 2,
          left: on ? 16 : 2,
          transition: 'left 140ms',
          boxShadow: '0 1px 2px rgba(0,0,0,0.2)',
        }}
      />
    </button>
    <span style={{ fontSize: 12.5, color: 'var(--text-2)' }}>{label}</span>
  </div>
);

const InfoBox = ({ icon, children }) => (
  <div
    style={{
      display: 'flex',
      gap: 9,
      padding: 11,
      background: 'var(--accent-soft)',
      border: '1px solid var(--accent-border)',
      borderRadius: 6,
      fontSize: 11.5,
      color: 'var(--accent-text)',
      lineHeight: 1.5,
    }}
  >
    <Icon name={icon} size={14} style={{ flexShrink: 0, marginTop: 1 }} />
    <div>{children}</div>
  </div>
);

window.SettingsPanel = SettingsPanel;
