/* global React */
// Tiny icon set - 16px stroke icons, original geometry.

const Icon = ({ name, size = 14, stroke = 1.5, style = {}, ...rest }) => {
  const paths = ICONS[name];
  if (!paths) return null;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={stroke}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ flexShrink: 0, ...style }}
      {...rest}
    >
      {paths}
    </svg>
  );
};

const ICONS = {
  plus: <path d="M8 3.5v9M3.5 8h9" />,
  x: <path d="M4 4l8 8M12 4l-8 8" />,
  play: <path d="M5 3.5v9l7-4.5z" fill="currentColor" stroke="none" />,
  pause: <><path d="M5.5 3.5v9M10.5 3.5v9" /></>,
  stop: <rect x="4" y="4" width="8" height="8" rx="1" fill="currentColor" stroke="none" />,
  trash: <><path d="M3 4.5h10M6 4.5V3a1 1 0 011-1h2a1 1 0 011 1v1.5M5 4.5l.5 8a1 1 0 001 1h3a1 1 0 001-1l.5-8" /></>,
  download: <><path d="M8 2.5v8" /><path d="M4.5 7.5L8 11l3.5-3.5" /><path d="M3 13.5h10" /></>,
  rotate: <><path d="M13 8a5 5 0 11-1.5-3.5" /><path d="M13 2.5V5h-2.5" /></>,
  gear: <><circle cx="8" cy="8" r="2" /><path d="M8 1.5v1.5M8 13v1.5M14.5 8H13M3 8H1.5M12.6 3.4l-1 1M4.4 11.6l-1 1M12.6 12.6l-1-1M4.4 4.4l-1-1" /></>,
  search: <><circle cx="7" cy="7" r="4" /><path d="M10 10l3 3" /></>,
  folder: <path d="M2 4.5a1 1 0 011-1h3l1.5 1.5h5.5a1 1 0 011 1v5.5a1 1 0 01-1 1H3a1 1 0 01-1-1z" />,
  alert: <><path d="M8 2.5l6.5 11h-13z" /><path d="M8 6.5v3.5M8 12v.01" /></>,
  info: <><circle cx="8" cy="8" r="6" /><path d="M8 7v4M8 5v.01" /></>,
  check: <path d="M3 8.5L6.5 12 13 4.5" />,
  chevronDown: <path d="M4 6l4 4 4-4" />,
  chevronRight: <path d="M6 4l4 4-4 4" />,
  link: <><path d="M9 6.5l.5-.5a3 3 0 014 4l-2 2a3 3 0 01-4 0" /><path d="M7 9.5l-.5.5a3 3 0 01-4-4l2-2a3 3 0 014 0" /></>,
  dots: <><circle cx="3.5" cy="8" r="1" fill="currentColor" /><circle cx="8" cy="8" r="1" fill="currentColor" /><circle cx="12.5" cy="8" r="1" fill="currentColor" /></>,
  sun: <><circle cx="8" cy="8" r="2.5" /><path d="M8 1.5v1.5M8 13v1.5M14.5 8H13M3 8H1.5M12.6 3.4l-1 1M4.4 11.6l-1 1M12.6 12.6l-1-1M4.4 4.4l-1-1" /></>,
  moon: <path d="M13 9.5A5.5 5.5 0 116.5 3a4.5 4.5 0 006.5 6.5z" />,
  sliders: <><path d="M3 4.5h10M3 11.5h10" /><circle cx="6" cy="4.5" r="1.5" fill="var(--surface)" /><circle cx="10" cy="11.5" r="1.5" fill="var(--surface)" /></>,
  shield: <path d="M8 1.5l5 2v4c0 3-2.2 5.5-5 6.5-2.8-1-5-3.5-5-6.5v-4z" />,
  eye: <><path d="M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8s-2.5 4.5-6.5 4.5S1.5 8 1.5 8z" /><circle cx="8" cy="8" r="1.8" /></>,
  list: <><path d="M5 4.5h9M5 8h9M5 11.5h9" /><circle cx="2.5" cy="4.5" r=".7" fill="currentColor" /><circle cx="2.5" cy="8" r=".7" fill="currentColor" /><circle cx="2.5" cy="11.5" r=".7" fill="currentColor" /></>,
  close: <path d="M4 4l8 8M12 4l-8 8" />,
};

// Tiny corner-pin lock for window chrome
const TrafficIcon = ({ kind }) => null;

window.Icon = Icon;
