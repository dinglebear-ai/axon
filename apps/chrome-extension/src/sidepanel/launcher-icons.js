/* ============================================================
 * Axon launcher — icon set
 * Lucide-style line icons (currentColor, 1.6 stroke) + Axon neuron mark.
 * Ported from the design handoff (Reference/axon/icons.jsx) to plain DOM.
 * ============================================================ */

const SVG_NS = "http://www.w3.org/2000/svg";

const ICON_PATHS = {
  search: '<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>',
  send: '<path d="M14.5 4.5 3 9.5l7 2 2 7 5-11.5z"/>',
  settings: '<line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="18" x2="20" y2="18"/><circle cx="9" cy="6" r="2.2" fill="var(--axon-surface)"/><circle cx="15" cy="12" r="2.2" fill="var(--axon-surface)"/><circle cx="8" cy="18" r="2.2" fill="var(--axon-surface)"/>',
  x: '<path d="M6 6l12 12M18 6 6 18"/>',
  copy: '<rect x="9" y="9" width="11" height="11" rx="2.5"/><path d="M5 15V5a2 2 0 0 1 2-2h8"/>',
  check: '<path d="M5 12.5 10 17l9-10"/>',
  refresh: '<path d="M3 12a9 9 0 0 1 15-6.7L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-15 6.7L3 16"/><path d="M3 21v-5h5"/>',
  external: '<path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M19 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h6"/>',
  clock: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>',
  history: '<path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/><path d="M12 7.5V12l3.5 2"/>',
  chevronRight: '<path d="m9 5 7 7-7 7"/>',
  arrowLeft: '<path d="M19 12H5"/><path d="m12 19-7-7 7-7"/>',
  chevronDown: '<path d="m6 9 6 6 6-6"/>',
  arrowUp: '<path d="M12 19V5"/><path d="m6 11 6-6 6 6"/>',
  arrowDown: '<path d="M12 5v14"/><path d="m6 13 6 6 6-6"/>',
  enter: '<path d="M9 10 5 14l4 4"/><path d="M5 14h11a4 4 0 0 0 4-4V6"/>',
  command: '<path d="M15 6a3 3 0 1 1 3 3h-3V6Z"/><path d="M9 6a3 3 0 1 0-3 3h3V6Z"/><path d="M9 18a3 3 0 1 1-3-3h3v3Z"/><path d="M15 18a3 3 0 1 0 3-3h-3v3Z"/>',
  // operations
  scrape: '<path d="M14 3v4a1 1 0 0 0 1 1h4"/><path d="M5 3h9l5 5v11a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z"/><path d="M12 11v6"/><path d="m9 14 3 3 3-3"/>',
  crawl: '<circle cx="6" cy="6" r="2.4"/><circle cx="18" cy="6" r="2.4"/><circle cx="12" cy="18" r="2.4"/><path d="M8 7.2 10.8 16M16 7.2 13.2 16M8.3 6h7.4"/>',
  map: '<path d="M4 5h16"/><path d="M4 5v14"/><path d="M8 10h12"/><path d="M8 10v9"/><path d="M12 15h8"/><circle cx="4" cy="5" r="1.4" fill="currentColor"/><circle cx="8" cy="10" r="1.4" fill="currentColor"/><circle cx="12" cy="15" r="1.4" fill="currentColor"/>',
  summarize: '<path d="M4 6h16"/><path d="M4 11h11"/><path d="M4 16h16"/><path d="M4 21h8"/>',
  ask: '<path d="M9.5 9a2.5 2.5 0 1 1 3.6 2.2c-.8.4-1.1 1-1.1 1.8v.5"/><circle cx="12" cy="17" r="0.6" fill="currentColor"/><circle cx="12" cy="12" r="9"/>',
  // status / misc
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3a14 14 0 0 1 0 18 14 14 0 0 1 0-18Z"/>',
  link: '<path d="M9 15 15 9"/><path d="M11 6.5 13 4.5a4 4 0 0 1 6 6l-2 2"/><path d="M13 17.5 11 19.5a4 4 0 0 1-6-6l2-2"/>',
  server: '<rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><circle cx="7" cy="7.5" r="0.7" fill="currentColor"/><circle cx="7" cy="16.5" r="0.7" fill="currentColor"/>',
  database: '<ellipse cx="12" cy="6" rx="8" ry="3"/><path d="M4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6"/><path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  activity: '<path d="M3 12h4l3 8 4-16 3 8h4"/>',
  alert: '<path d="M12 3 2 20h20L12 3Z"/><path d="M12 9v5"/><circle cx="12" cy="17.5" r="0.6" fill="currentColor"/>',
  layers: '<path d="m12 3 9 5-9 5-9-5 9-5Z"/><path d="m3 13 9 5 9-5"/>',
  zap: '<path d="M13 3 4 14h7l-1 7 9-11h-7l1-7Z"/>',
  dot: '<circle cx="12" cy="12" r="4" fill="currentColor" stroke="none"/>',
  file: '<path d="M14 3v4a1 1 0 0 0 1 1h4"/><path d="M5 3h9l5 5v11a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z"/>',
  hash: '<path d="M9 4 7 20M17 4l-2 16M5 9h15M4 15h15"/>',
  pin: '<path d="M12 21s7-5.5 7-11a7 7 0 1 0-14 0c0 5.5 7 11 7 11Z"/><circle cx="12" cy="10" r="2.4"/>',
  terminal: '<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M7 9l3 3-3 3"/><path d="M13 15h4"/>',
  shield: '<path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6l8-3Z"/>',
  sparkles: '<path d="M12 4l1.5 4.2L18 10l-4.5 1.8L12 16l-1.5-4.2L6 10l4.5-1.8L12 4Z"/><path d="M19 14l.7 1.8L21 17l-1.3.6L19 19l-.7-1.4L17 17l1.3-1.2L19 14Z"/>',
  brain: '<path d="M9 4a3 3 0 0 0-3 3 3 3 0 0 0-1 5.8V15a3 3 0 0 0 4 2.8A3 3 0 0 0 12 19V5a3 3 0 0 0-3-1Z"/><path d="M15 4a3 3 0 0 1 3 3 3 3 0 0 1 1 5.8V15a3 3 0 0 1-4 2.8A3 3 0 0 1 12 19"/>',
  // operations — extended action surface
  camera: '<path d="M4 8h3l1.5-2.2h7L17 8h3a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1Z"/><circle cx="12" cy="13" r="3.4"/>',
  diff: '<path d="M12 3v18"/><rect x="3" y="6" width="6" height="12" rx="1.4"/><rect x="15" y="6" width="6" height="12" rx="1.4"/>',
  braces: '<path d="M8 4c-2 0-2 2-2 4s0 3-2 4c2 1 2 2 2 4s0 4 2 4"/><path d="M16 4c2 0 2 2 2 4s0 3 2 4c-2 1-2 2-2 4s0 4-2 4"/>',
  box: '<path d="m12 3 8 4.5v9L12 21l-8-4.5v-9L12 3Z"/><path d="m4 7.5 8 4.5 8-4.5"/><path d="M12 12v9"/>',
  compass: '<circle cx="12" cy="12" r="9"/><path d="m15.5 8.5-2 5-5 2 2-5 5-2Z"/>',
  target: '<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="4.5"/><circle cx="12" cy="12" r="1" fill="currentColor"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2.5h8a2 2 0 0 1 2 2V18a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z"/>',
  beaker: '<path d="M9 3h6"/><path d="M10 3v6.5L5.5 17a2 2 0 0 0 1.8 3h9.4a2 2 0 0 0 1.8-3L14 9.5V3"/><path d="M8 14h8"/>',
  barChart: '<path d="M4 20V10"/><path d="M10 20V4"/><path d="M16 20v-7"/><path d="M3 20h18"/>',
  palette: '<path d="M12 3a9 9 0 1 0 0 18c1.4 0 2-1 2-2 0-1.4-1-1.6-1-3 0-.8.7-1.5 1.5-1.5H17a4 4 0 0 0 4-4c0-4-4-7.5-9-7.5Z"/><circle cx="8" cy="11" r="1.1" fill="currentColor" stroke="none"/><circle cx="12" cy="8" r="1.1" fill="currentColor" stroke="none"/><circle cx="16" cy="10" r="1.1" fill="currentColor" stroke="none"/>',
  plug: '<path d="M9 3v5"/><path d="M15 3v5"/><path d="M7 8h10v3a5 5 0 0 1-10 0V8Z"/><path d="M12 16v5"/>',
};

/* Build a lucide-style line-icon <svg> element. */
function iconSvg(name, opts = {}) {
  const path = ICON_PATHS[name];
  const svg = document.createElementNS(SVG_NS, "svg");
  const size = opts.size || 16;
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", String(opts.stroke || 1.6));
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  svg.style.flex = "0 0 auto";
  if (opts.color) svg.style.color = opts.color;
  if (path) svg.innerHTML = path;
  return svg;
}

/* Axon neuron mark — vertical signal spine, 4 nodes ramping crawl→retrieve. */
function axonMark(size = 22, pulse = false) {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("viewBox", "0 0 64 64");
  svg.setAttribute("fill", "none");
  svg.setAttribute("aria-hidden", "true");
  if (pulse) svg.classList.add("axon-pulse");
  svg.innerHTML = `
    <g stroke="var(--aurora-border-strong)" stroke-width="2" stroke-linecap="round">
      <path d="M22 9 Q28 14 31 17"/>
      <path d="M32 7 L32 16"/>
      <path d="M42 9 Q36 14 33 17"/>
    </g>
    <line x1="32" y1="22" x2="32" y2="42" stroke="var(--aurora-border-strong)" stroke-width="2" stroke-dasharray="2.5 3.5"/>
    <circle class="axon-node" style="animation-delay:0s" cx="32" cy="20" r="5.2" fill="var(--aurora-border-strong)" stroke="var(--aurora-accent-strong)" stroke-width="1.8"/>
    <circle class="axon-node" style="animation-delay:0.16s" cx="32" cy="30" r="5.2" fill="var(--aurora-accent-deep)" stroke="var(--aurora-accent-strong)" stroke-width="1.8"/>
    <circle class="axon-node" style="animation-delay:0.32s" cx="32" cy="40" r="5.2" fill="var(--aurora-accent-primary)" stroke="var(--aurora-accent-strong)" stroke-width="1.8"/>
    <circle class="axon-node" style="animation-delay:0.48s" cx="32" cy="50" r="5.2" fill="var(--aurora-accent-strong)"/>
    <circle cx="32" cy="50" r="8" fill="none" stroke="var(--aurora-accent-strong)" stroke-width="1.2" opacity="0.4"/>
    <g stroke="var(--aurora-accent-strong)" stroke-width="2" stroke-linecap="round">
      <path d="M28 53 Q23 58 19 62"/>
      <path d="M32 54 L32 62"/>
      <path d="M36 53 Q41 58 45 62"/>
    </g>`;
  return svg;
}

window.AxonIcons = { iconSvg, axonMark, ICON_PATHS };
