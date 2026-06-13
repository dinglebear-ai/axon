import type { ActionHelp } from "@/lib/actionHelp";

function isActionHelp(value: unknown): value is ActionHelp {
  if (!value || typeof value !== "object") return false;
  const item = value as Record<string, unknown>;
  const route = item.route;
  if (route === null || typeof route !== "object") return false;
  const routeRecord = route as Record<string, unknown>;
  const stringArray = (candidate: unknown) => Array.isArray(candidate) && candidate.every((entry) => typeof entry === "string");
  return (
    typeof item.title === "string" &&
    typeof item.subcommand === "string" &&
    stringArray(item.aliases) &&
    typeof item.description === "string" &&
    typeof item.usage === "string" &&
    typeof item.category === "string" &&
    typeof item.output === "string" &&
    typeof item.async === "boolean" &&
    (routeRecord.method === "GET" || routeRecord.method === "POST" || routeRecord.method === "DELETE") &&
    typeof routeRecord.path === "string" &&
    stringArray(item.parameters) &&
    stringArray(item.options)
  );
}

function payloadRecord(payload: unknown): Record<string, unknown> {
  return payload && typeof payload === "object" ? (payload as Record<string, unknown>) : {};
}

export interface HelpResultViewProps {
  payload: unknown;
  fallbackText: string;
}

export function HelpResultView({ payload, fallbackText }: HelpResultViewProps) {
  const body = payloadRecord(payload);
  const target = isActionHelp(body.target) ? body.target : undefined;
  const catalog = Array.isArray(body.catalog) ? body.catalog.filter(isActionHelp) : [];
  const unknownTarget = typeof body.unknownTarget === "string" ? body.unknownTarget : "";

  if (!target && catalog.length === 0) {
    if (fallbackText.trim()) {
      return <pre className="result-code">{fallbackText}</pre>;
    }
    return (
      <div className="output-body help-result help-result-invalid aurora-scrollbar" role="alert">
        <header className="help-header">
          <span className="help-route">palette://help</span>
          <h2>Help payload is malformed</h2>
          <p>The palette could not render structured help from this run.</p>
        </header>
      </div>
    );
  }

  if (target) {
    return (
      <div className="output-body help-result aurora-scrollbar">
        <header className="help-header">
          <span className="help-route">{target.route.method} {target.route.path}</span>
          <h2>{target.title}</h2>
          <p>{target.description}</p>
        </header>
        <section className="help-section">
          <h3>Usage</h3>
          <code>{target.usage}</code>
        </section>
        <section className="help-section">
          <h3>Parameters</h3>
          <ul>{target.parameters.map((item) => <li key={item}>{item}</li>)}</ul>
        </section>
        <section className="help-section">
          <h3>Options</h3>
          <ul>{target.options.map((item) => <li key={item}>{item}</li>)}</ul>
        </section>
        {target.aliases.length > 0 && (
          <section className="help-section help-aliases">
            <h3>Aliases</h3>
            <div>{target.aliases.map((alias) => <code key={alias}>{alias}</code>)}</div>
          </section>
        )}
      </div>
    );
  }

  const groups = new Map<string, ActionHelp[]>();
  for (const item of catalog) {
    const list = groups.get(item.category) ?? [];
    list.push(item);
    groups.set(item.category, list);
  }

  return (
    <div className="output-body help-result aurora-scrollbar">
      <header className="help-header">
        <span className="help-route">palette://help</span>
        <h2>Axon Palette Help</h2>
        <p>Use help &lt;action&gt;, &lt;action&gt; help, or the selected action question mark.</p>
        {unknownTarget ? <p className="help-warning">No matching action: <code>{unknownTarget}</code></p> : null}
      </header>
      {[...groups.entries()].map(([category, items]) => (
        <section className="help-catalog-section" key={category}>
          <h3>{category}</h3>
          <div className="help-catalog">
            {items.map((item) => (
              <article className="help-catalog-item" key={item.subcommand}>
                <code>{item.subcommand}</code>
                <span>{item.description}</span>
              </article>
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
