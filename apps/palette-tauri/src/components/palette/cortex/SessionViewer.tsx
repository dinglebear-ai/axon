import type { CortexSessionEvent } from "@/lib/clients/cortexClient";
import { safeText, visibleWindow } from "@/lib/cortex/viewModel";

export function SessionViewer({
  events,
  scrollTop,
  onScroll,
}: {
  events: CortexSessionEvent[];
  scrollTop: number;
  onScroll: (value: number) => void;
}) {
  const windowed = visibleWindow(events, scrollTop, 84, 560);
  return (
    <section
      aria-label="Rendered Cortex session"
      className="cortex-list cortex-session-list"
      onScroll={(event) => onScroll(event.currentTarget.scrollTop)}
    >
      <div style={{ height: windowed.top }} />
      {windowed.rows.map((item) => (
        <article key={item.position} data-kind={item.kind}>
          <header>
            <strong>{item.kind}</strong>
            <time>{safeText(item.timestamp, 100)}</time>
            {item.redacted && <span>redacted</span>}
          </header>
          <p>{safeText(item.text, 16000)}</p>
          {item.parse_warning && <small>Parse warning: {safeText(item.parse_warning, 500)}</small>}
        </article>
      ))}
      <div style={{ height: windowed.bottom }} />
    </section>
  );
}
