import { useEffect, useMemo, useRef, useState } from "react";
import {
  assertCompatibleIdentity,
  type BackendProfile,
  type ProductIdentity,
} from "@/lib/backendProfiles/model";
import {
  CORTEX_GRAPH_ENTITY_TYPES,
  CortexClient,
  type CortexFleetHost,
  type CortexGraphEntityType,
  type CortexGraphResult,
  type CortexLog,
  type CortexSessionEvent,
  type CortexSessionIdentity,
  type CortexSessionSearchEntry,
} from "@/lib/clients/cortexClient";
import { followCortexStream } from "@/lib/cortex/streams/nativeStream";
import {
  boundedAppend,
  boundedByItemsAndBytes,
  CORTEX_CAPABILITY,
  type CortexTab,
  capabilityAvailable,
  safeText,
  visibleWindow,
} from "@/lib/cortex/viewModel";
import { SessionViewer } from "./SessionViewer";
import { TailControls } from "./TailControls";

const tabs: CortexTab[] = ["logs", "sessions", "fleet", "graph", "correlate"];

export function CortexWorkspace({ profile }: { profile: BackendProfile }) {
  const client = useMemo(() => new CortexClient(profile), [profile]);
  const [tab, setTab] = useState<CortexTab>("logs");
  const [identity, setIdentity] = useState<ProductIdentity | null>(null);
  const [query, setQuery] = useState("");
  const [graphEntityType, setGraphEntityType] = useState<CortexGraphEntityType>("host");
  const [rows, setRows] = useState<CortexLog[]>([]);
  const [fleet, setFleet] = useState<CortexFleetHost[]>([]);
  const [graph, setGraph] = useState<CortexGraphResult | null>(null);
  const [sessionEvents, setSessionEvents] = useState<CortexSessionEvent[]>([]);
  const [sessionMatches, setSessionMatches] = useState<CortexSessionSearchEntry[]>([]);
  const [session, setSession] = useState<CortexSessionIdentity>({
    project: "",
    tool: "",
    sessionId: "",
    host: "",
  });
  const [following, setFollowing] = useState(false);
  const [paused, setPaused] = useState(false);
  const [streamCursor, setStreamCursor] = useState<string | null>(null);
  const [cursor, setCursor] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [scrollTop, setScrollTop] = useState(0);
  const generation = useRef(0);
  const controller = useRef<AbortController | null>(null);
  const streamController = useRef<AbortController | null>(null);
  const available = capabilityAvailable(identity, tab);

  useEffect(() => {
    const abort = new AbortController();
    setIdentity(null);
    client
      .identity(abort.signal)
      .then((result) => setIdentity(assertCompatibleIdentity(profile, result.payload)))
      .catch((error) => {
        if ((error as Error).name !== "AbortError") setNotice(String(error));
      });
    return () => abort.abort();
  }, [client, profile]);
  useEffect(
    () => () => {
      controller.current?.abort();
      streamController.current?.abort();
    },
    [],
  );
  useEffect(() => {
    if (!client.profile.id || !tab) return;
    controller.current?.abort();
    setRows([]);
    setFleet([]);
    setGraph(null);
    setSessionEvents([]);
    setSessionMatches([]);
    setFollowing(false);
    setPaused(false);
    streamController.current?.abort();
    setCursor(null);
    setNotice(null);
  }, [client, tab]);

  async function load(append = false) {
    controller.current?.abort();
    const abort = new AbortController();
    controller.current = abort;
    const mine = ++generation.current;
    setLoading(true);
    setNotice(null);
    try {
      if (tab === "logs") {
        const result = await client.logs(
          { query, cursor: append ? (cursor ?? undefined) : undefined, limit: 100 },
          abort.signal,
        );
        if (mine !== generation.current) return;
        setRows((old) =>
          append ? boundedAppend(old, result.payload.logs) : result.payload.logs.slice(0, 500),
        );
        setCursor(result.payload.next_cursor ?? null);
        if (result.payload.truncated)
          setNotice("Results were truncated by Cortex; continue with the server cursor.");
      } else if (tab === "sessions") {
        if (query.trim()) {
          const result = await client.searchSessions(query, abort.signal);
          if (mine !== generation.current) return;
          setSessionMatches(result.payload.sessions.slice(0, 50));
          if (result.payload.truncated || result.payload.candidate_window_truncated)
            setNotice("Cortex bounded the search window; refine the query for complete results.");
          return;
        }
        const result = await client.renderedSession(
          session,
          append ? (cursor ?? undefined) : undefined,
          abort.signal,
        );
        if (mine !== generation.current) return;
        setSessionEvents((old) =>
          append
            ? boundedByItemsAndBytes(
                old,
                result.payload.events,
                (item) => item.text.length * 2 + 256,
              )
            : result.payload.events,
        );
        setCursor(result.payload.has_more ? result.payload.next_cursor : null);
        setStreamCursor(result.payload.next_cursor);
        if (result.payload.truncated_by_bytes)
          setNotice(
            "Session page reached Cortex's byte bound; continue from its committed cursor.",
          );
      } else if (tab === "fleet") {
        const result = await client.fleet(abort.signal);
        if (mine === generation.current) setFleet(result.payload.hosts);
      } else if (tab === "graph") {
        const result = await client.graph(
          { entityType: graphEntityType, key: query },
          abort.signal,
        );
        if (mine === generation.current) setGraph(result.payload);
      } else {
        const result = await client.correlate(
          { query, cursor: append ? (cursor ?? undefined) : undefined, limit: 100 },
          abort.signal,
        );
        if (mine !== generation.current) return;
        setRows((old) =>
          append ? boundedAppend(old, result.payload.logs) : result.payload.logs.slice(0, 500),
        );
        setCursor(result.payload.next_cursor ?? null);
        if (!result.payload.logs.length) setNotice("No correlation evidence matched this anchor.");
      }
    } catch (error) {
      if ((error as Error).name !== "AbortError" && mine === generation.current)
        setNotice(String(error));
    } finally {
      if (mine === generation.current) setLoading(false);
    }
  }

  function startFollowing(kind: "logs" | "sessions") {
    streamController.current?.abort();
    const abort = new AbortController();
    streamController.current = abort;
    const mine = ++generation.current;
    setFollowing(true);
    setPaused(false);
    setNotice("Connecting to durable Cortex stream…");
    const params: Record<string, string> = {};
    if (streamCursor) params.cursor = streamCursor;
    if (kind === "sessions")
      Object.assign(params, {
        project: session.project,
        tool: session.tool,
        session_id: session.sessionId,
        host: session.host,
      });
    void followCortexStream(
      profile,
      kind,
      params,
      mine,
      (message) => {
        if (mine !== generation.current || paused) return;
        if (message.id) setStreamCursor(message.id);
        if (message.event === "snapshot") {
          setNotice("Live at Cortex's committed snapshot.");
          return;
        }
        if (["token_expired", "retention_gap", "cursor_expired"].includes(message.event)) {
          setFollowing(false);
          setNotice(
            "The durable cursor can no longer resume. Reload the bounded snapshot to resync.",
          );
          abort.abort();
          return;
        }
        if (message.event === "overload") {
          setFollowing(false);
          setNotice("Cortex ended the tail under load. Resume when ready.");
          return;
        }
        const data = message.data as Record<string, unknown>;
        if (kind === "sessions" && message.event === "session") {
          const event: CortexSessionEvent = {
            position: Number(data.position),
            timestamp: String(data.timestamp ?? ""),
            kind: String(
              (data.metadata as Record<string, unknown> | undefined)?.event_kind ?? "unknown",
            ) as CortexSessionEvent["kind"],
            text: String(data.message ?? ""),
            redacted: Boolean((data.metadata as Record<string, unknown> | undefined)?.redacted),
            parse_warning: data.parseWarning ? String(data.parseWarning) : undefined,
          };
          setSessionEvents((old) =>
            boundedByItemsAndBytes(old, [event], (item) => item.text.length * 2 + 256),
          );
        } else if (kind === "logs" && message.event === "log") {
          const log: CortexLog = {
            id: Number(data.position),
            timestamp: String(data.timestamp ?? ""),
            hostname: String(data.host ?? ""),
            severity: String(data.severity ?? ""),
            app_name: data.app == null ? null : String(data.app),
            message: String(data.message ?? ""),
          };
          setRows((old) =>
            boundedByItemsAndBytes(old, [log], (item) => item.message.length * 2 + 256),
          );
        }
      },
      abort.signal,
    )
      .catch((error) => {
        if (!abort.signal.aborted && mine === generation.current) setNotice(String(error));
      })
      .finally(() => {
        if (mine === generation.current) setFollowing(false);
      });
  }

  const windowed = visibleWindow(rows, scrollTop);
  return (
    <main className="cortex-workspace">
      <header>
        <div>
          <p className="cortex-eyebrow">Cortex observability</p>
          <h1>{profile.label}</h1>
        </div>
        <span className="cortex-server">{identity?.server_id ?? "Identity unavailable"}</span>
      </header>
      <nav aria-label="Cortex workspace">
        {tabs.map((item) => (
          <button
            type="button"
            key={item}
            aria-current={tab === item ? "page" : undefined}
            onClick={() => setTab(item)}
          >
            {item}
          </button>
        ))}
      </nav>
      {!identity ? (
        <section role="status" className="cortex-empty">
          <h2>Verifying Cortex identity</h2>
          <p>
            Queries remain disabled until the profile, API major, server pin, and capabilities are
            verified.
          </p>
        </section>
      ) : !available ? (
        <section role="status" className="cortex-empty">
          <h2>{tab} unavailable</h2>
          <p>
            The selected Cortex server did not advertise <code>{CORTEX_CAPABILITY[tab]}</code>. No
            request was sent.
          </p>
        </section>
      ) : (
        <>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void load(false);
            }}
          >
            <label htmlFor="cortex-query">
              {tab === "sessions"
                ? "Session search and identity"
                : tab === "graph"
                  ? "Cortex-qualified entity key"
                  : "Filter or correlation anchor"}
            </label>
            <div>
              {tab === "sessions" ? (
                <div className="cortex-session-fields">
                  <input
                    id="cortex-query"
                    aria-label="Search sessions"
                    placeholder="Search transcript text"
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                  />
                  {(["project", "tool", "sessionId", "host"] as const).map((field) => (
                    <input
                      key={field}
                      aria-label={field === "sessionId" ? "Session ID" : field}
                      placeholder={field}
                      value={session[field]}
                      onChange={(event) => setSession({ ...session, [field]: event.target.value })}
                      required={!query.trim()}
                    />
                  ))}
                </div>
              ) : tab === "graph" ? (
                <div className="cortex-session-fields">
                  <select
                    aria-label="Graph entity type"
                    value={graphEntityType}
                    onChange={(event) =>
                      setGraphEntityType(event.target.value as CortexGraphEntityType)
                    }
                  >
                    {CORTEX_GRAPH_ENTITY_TYPES.map((entityType) => (
                      <option key={entityType} value={entityType}>
                        {entityType.replaceAll("_", " ")}
                      </option>
                    ))}
                  </select>
                  <input
                    id="cortex-query"
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                    autoComplete="off"
                  />
                </div>
              ) : (
                <input
                  id="cortex-query"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  autoComplete="off"
                />
              )}
              <button type="submit" disabled={loading}>
                {loading
                  ? "Loading…"
                  : tab === "fleet"
                    ? "Refresh"
                    : tab === "sessions" && query.trim()
                      ? "Search"
                      : "Run"}
              </button>
            </div>
          </form>
          {notice && (
            <p role="status" className="cortex-notice">
              {notice}
            </p>
          )}
          {tab === "fleet" && (
            <section aria-label="Fleet hosts" className="cortex-grid">
              {fleet.map((host) => (
                <article key={host.hostname}>
                  <h2>{safeText(host.hostname, 200)}</h2>
                  <strong data-status={host.status}>{safeText(host.status, 80)}</strong>
                  <p>
                    {host.last_seen_at
                      ? `Last seen ${safeText(host.last_seen_at, 100)}`
                      : "No heartbeat observed"}
                  </p>
                  {host.degraded_reasons?.map((reason) => (
                    <p key={reason}>{safeText(reason, 300)}</p>
                  ))}
                </article>
              ))}
            </section>
          )}
          {(tab === "logs" || tab === "correlate") && (
            <section
              aria-label={tab === "logs" ? "Cortex logs" : "Correlation evidence"}
              className="cortex-list"
              onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
            >
              <div style={{ height: windowed.top }} />
              {windowed.rows.map((log) => (
                <article key={log.id}>
                  <time>{safeText(log.timestamp, 100)}</time>
                  <span>{safeText(log.hostname, 120)}</span>
                  <strong>{safeText(log.severity, 40)}</strong>
                  <p>{safeText(log.message)}</p>
                  {log.correlation_id && (
                    <button
                      type="button"
                      onClick={() => {
                        setTab("correlate");
                        setQuery(log.correlation_id ?? "");
                      }}
                    >
                      Correlation {safeText(log.correlation_id, 120)}
                    </button>
                  )}
                  {log.parse_warnings?.map((warning) => (
                    <small key={warning}>Parse warning: {safeText(warning, 300)}</small>
                  ))}
                </article>
              ))}
              <div style={{ height: windowed.bottom }} />
            </section>
          )}
          {tab === "sessions" && (
            <>
              {sessionMatches.length > 0 && (
                <section className="cortex-session-matches" aria-label="Session search results">
                  {sessionMatches.map((match) => (
                    <button
                      type="button"
                      key={match.session_key}
                      onClick={() => {
                        setSession({
                          project: match.project,
                          tool: match.tool,
                          sessionId: match.session_id,
                          host: match.hostname,
                        });
                        setQuery("");
                        setSessionMatches([]);
                      }}
                    >
                      <strong>
                        {safeText(match.project, 200)} · {safeText(match.tool, 80)}
                      </strong>
                      <span>{safeText(match.best_snippet ?? match.session_id, 500)}</span>
                      <small>
                        {match.match_count} matches · {match.event_count} events
                      </small>
                    </button>
                  ))}
                </section>
              )}
              <SessionViewer events={sessionEvents} scrollTop={scrollTop} onScroll={setScrollTop} />
            </>
          )}
          {(tab === "logs" || tab === "sessions") &&
            ((tab === "logs" && rows.length > 0) ||
              (tab === "sessions" && sessionEvents.length > 0)) && (
              <TailControls
                following={following}
                paused={paused}
                onFollow={() => startFollowing(tab)}
                onPause={() => {
                  setPaused(true);
                  setFollowing(false);
                  streamController.current?.abort();
                }}
                onCancel={() => {
                  setFollowing(false);
                  setPaused(false);
                  setStreamCursor(null);
                  streamController.current?.abort();
                }}
                onResume={() => startFollowing(tab)}
              />
            )}
          {graph && tab === "graph" && (
            <section aria-label="Cortex graph" className="cortex-graph">
              <p>
                {graph.projection_status ?? "Projection status unavailable"} · watermark{" "}
                {safeText(graph.source_watermark ?? "unknown", 120)}
              </p>
              {graph.degraded_reason && (
                <p role="status">Degraded: {safeText(graph.degraded_reason, 500)}</p>
              )}
              {graph.relationships?.slice(0, 100).map((edge) => (
                <article key={edge.id}>
                  <h2>{safeText(edge.relationship_type, 120)}</h2>
                  <p>
                    {safeText(edge.src_entity?.display_label ?? edge.src_entity?.canonical_key)} →{" "}
                    {safeText(edge.dst_entity?.display_label ?? edge.dst_entity?.canonical_key)}
                  </p>
                  <small>
                    {edge.evidence_count} evidence records · {Math.round(edge.confidence * 100)}%
                    confidence
                  </small>
                </article>
              ))}
            </section>
          )}
          {cursor && (tab === "logs" || tab === "sessions" || tab === "correlate") && (
            <button
              type="button"
              className="cortex-more"
              disabled={loading}
              onClick={() => void load(true)}
            >
              Load next page
            </button>
          )}
        </>
      )}
    </main>
  );
}
