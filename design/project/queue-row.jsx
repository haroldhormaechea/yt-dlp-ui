/* global React, Icon, Thumbnail */
// QueueRow — renders one row in all possible states.

const STATUS_LABEL = {
  queued: 'Queued',
  in_flight: 'Downloading',
  done: 'Done',
  cancelled: 'Cancelled',
  error: 'Error',
  cancelling: 'Cancelling…',
  waiting_on_user: 'Waiting on you',
};
const STATUS_CLASS = {
  queued: 'badge-queued',
  in_flight: 'badge-inflight',
  done: 'badge-done',
  cancelled: 'badge-cancelled',
  error: 'badge-error',
  cancelling: 'badge-cancelling',
  waiting_on_user: 'badge-waiting',
};

const ProgressBar = ({ pct = 0, status }) => {
  const isInFlight = status === 'in_flight';
  const isCancelled = status === 'cancelled';
  const isError = status === 'error';
  const color = isCancelled
    ? 'var(--text-disabled)'
    : isError
    ? 'var(--danger)'
    : 'var(--accent)';
  return (
    <div
      style={{
        height: 4,
        background: 'var(--surface-3)',
        borderRadius: 2,
        overflow: 'hidden',
        position: 'relative',
      }}
    >
      <div
        style={{
          width: `${Math.max(0, Math.min(100, pct))}%`,
          height: '100%',
          background: color,
          transition: 'width .25s ease',
          opacity: isInFlight ? 1 : isCancelled ? 0.5 : 1,
        }}
      />
      {isInFlight && (
        <div
          style={{
            position: 'absolute',
            inset: 0,
            backgroundImage:
              'repeating-linear-gradient(45deg, rgba(255,255,255,0.18) 0 6px, transparent 6px 12px)',
            backgroundSize: '17px 17px',
            animation: 'qrShimmer 1.2s linear infinite',
            mixBlendMode: 'overlay',
          }}
        />
      )}
    </div>
  );
};

const QueueRow = ({ item, onAction, hovered }) => {
  const {
    id, title, url, source, status, pct, speed, eta, size, downloaded,
    duration, error, seed, fetching,
  } = item;

  const muted = status === 'cancelled' || status === 'done' || status === 'error';
  const showProgress =
    status === 'in_flight' || status === 'cancelled' || status === 'cancelling';

  const titleEl = fetching ? (
    <span style={{ color: 'var(--text-3)', fontStyle: 'italic' }}>Fetching…</span>
  ) : (
    title
  );

  return (
    <div
      data-row-id={id}
      style={{
        display: 'flex',
        gap: 14,
        padding: '14px 16px',
        borderBottom: '1px solid var(--divider)',
        background: hovered ? 'var(--surface-2)' : 'transparent',
        opacity: status === 'cancelled' ? 0.62 : 1,
        position: 'relative',
        alignItems: 'flex-start',
      }}
    >
      <Thumbnail source={source} seed={seed || id} duration={duration} width={104} height={58} />

      <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 5 }}>
        {/* Row 1: title + badge */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <div
            style={{
              fontSize: 13.5,
              fontWeight: 500,
              color: muted ? 'var(--text-2)' : 'var(--text)',
              textDecoration: status === 'cancelled' ? 'line-through' : 'none',
              textDecorationColor: 'var(--text-3)',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
              flex: 1,
              minWidth: 0,
            }}
          >
            {status === 'error' && !fetching && (
              <span style={{ color: 'var(--danger)', marginRight: 6, display: 'inline-flex', verticalAlign: '-2px' }}>
                <Icon name="alert" size={13} />
              </span>
            )}
            {titleEl}
          </div>
          <span className={`badge ${STATUS_CLASS[status]}`}>
            <span className="dot" />
            {STATUS_LABEL[status]}
          </span>
        </div>

        {/* Row 2: url */}
        <div
          className="mono"
          style={{
            color: 'var(--text-3)',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
          title={url}
        >
          {url}
        </div>

        {/* Row 3: progress (when relevant) */}
        {showProgress && (
          <div style={{ marginTop: 4, display: 'flex', flexDirection: 'column', gap: 5 }}>
            <ProgressBar pct={pct} status={status} />
            <div
              className="mono"
              style={{
                display: 'flex',
                gap: 14,
                color: status === 'cancelled' ? 'var(--text-3)' : 'var(--text-2)',
                fontSize: 11,
              }}
            >
              <span style={{ color: status === 'in_flight' ? 'var(--text)' : 'inherit', fontWeight: 500 }}>
                {pct.toFixed(1)}%
              </span>
              {downloaded && size && <span>{downloaded} / {size}</span>}
              {status === 'in_flight' && speed && <span>{speed}</span>}
              {status === 'in_flight' && eta && <span>ETA {eta}</span>}
              {status === 'cancelled' && <span style={{ fontStyle: 'italic' }}>stopped</span>}
            </div>
          </div>
        )}

        {/* Row 3 alt: error message */}
        {status === 'error' && error && (
          <div
            style={{
              marginTop: 4,
              fontSize: 11.5,
              color: 'var(--danger-text)',
              background: 'var(--danger-soft)',
              padding: '5px 8px',
              borderRadius: 4,
              lineHeight: 1.45,
            }}
          >
            {error}
          </div>
        )}

        {/* Row 3 alt: waiting */}
        {status === 'waiting_on_user' && (
          <div
            style={{
              marginTop: 4,
              fontSize: 11.5,
              color: 'var(--warning-text)',
              display: 'flex',
              alignItems: 'center',
              gap: 6,
            }}
          >
            <Icon name="info" size={12} />
            YouTube wants cookies — see dialog above.
          </div>
        )}

        {/* Row 3 alt: done meta */}
        {status === 'done' && (
          <div
            className="mono"
            style={{
              marginTop: 2,
              fontSize: 11,
              color: 'var(--text-3)',
              display: 'flex',
              gap: 14,
            }}
          >
            <span style={{ color: 'var(--success-text)' }}>
              <Icon name="check" size={11} style={{ verticalAlign: '-1px', marginRight: 3 }} />
              {size}
            </span>
            <span>saved to ~/Downloads/yt-dlp-ui</span>
          </div>
        )}
      </div>

      {/* Action column */}
      <div
        style={{
          display: 'flex',
          gap: 6,
          alignItems: 'flex-start',
          flexShrink: 0,
          paddingTop: 1,
        }}
      >
        <RowActions status={status} onAction={(a) => onAction && onAction(id, a)} />
      </div>
    </div>
  );
};

const RowActions = ({ status, onAction }) => {
  const disabled = status === 'cancelling';
  const Btn = ({ kind = 'ghost', label, icon, action, primary, danger }) => (
    <button
      className={`btn btn-sm ${primary ? 'btn-primary' : danger ? 'btn-danger' : kind === 'ghost' ? 'btn-ghost' : ''}`}
      onClick={() => onAction(action)}
      disabled={disabled}
      title={label}
    >
      {icon && <Icon name={icon} size={12} />}
      {label}
    </button>
  );

  if (status === 'queued') {
    return (
      <>
        <Btn label="Download" icon="download" action="start" primary />
        <Btn label="Cancel" action="cancel" />
        <Btn icon="x" label="Remove" action="remove" kind="ghost" />
      </>
    );
  }
  if (status === 'in_flight') {
    return (
      <>
        <Btn label="Cancel" icon="stop" action="cancel" />
        <Btn icon="x" label="Remove" action="remove" kind="ghost" />
      </>
    );
  }
  if (status === 'cancelling') {
    return (
      <>
        <Btn label="Cancelling…" action="noop" />
      </>
    );
  }
  if (status === 'cancelled') {
    return (
      <>
        <Btn label="Restart" icon="rotate" action="restart" />
        <Btn icon="x" label="Remove" action="remove" kind="ghost" />
      </>
    );
  }
  if (status === 'done' || status === 'error') {
    return <Btn icon="x" label="Remove" action="remove" kind="ghost" />;
  }
  if (status === 'waiting_on_user') {
    return (
      <>
        <Btn label="Cancel" action="cancel" />
        <Btn icon="x" label="Remove" action="remove" kind="ghost" />
      </>
    );
  }
  return null;
};

window.QueueRow = QueueRow;
