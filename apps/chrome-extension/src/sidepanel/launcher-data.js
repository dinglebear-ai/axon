/* ============================================================
 * Axon launcher — action surface + tone helpers
 * The full action surface (CLI / MCP), grouped by family.
 * Tones: cyan (fetch/read), orange (heavy/async jobs), rose (AI/LLM).
 * Ported from the design handoff (Reference/axon/data.jsx + parts.jsx).
 * ============================================================ */

/* action families shown as sections in browse */
const CATEGORIES = [
  { id: "read",     label: "Fetch & Read" },
  { id: "jobs",     label: "Crawl & Ingest" },
  { id: "discover", label: "Search & Discover" },
  { id: "reason",   label: "Reason" },
  { id: "system",   label: "System & Diagnostics" },
];

/* Each op: arg drives the launcher input mode —
 *   "url"            → prefill the active tab URL, run immediately, editable
 *   "query"/"focus"/"input"/"target"/"message" → text input, run on submit
 *   null + noArg     → run immediately, no input
 * dual: needs two URLs (diff). optionalArg/noArg: can run without an arg. */
const OPERATIONS = [
  /* ── Fetch & Read (cyan) ── */
  { id: "scrape",     name: "Scrape",     tone: "cyan",   icon: "scrape",   category: "read",     arg: "url",     argLabel: "URL",            argHint: "https://docs.rs/serde",                short: "Fetch one page → markdown" },
  { id: "retrieve",   name: "Retrieve",   tone: "cyan",   icon: "database", category: "read",     arg: "url",     argLabel: "URL",            argHint: "https://docs.rs/serde",                short: "Read a stored doc by URL" },
  { id: "screenshot", name: "Screenshot", tone: "cyan",   icon: "camera",   category: "read",     arg: "url",     argLabel: "URL",            argHint: "https://tokio.rs",                     short: "Capture a full-page PNG" },
  { id: "diff",       name: "Diff",       tone: "cyan",   icon: "diff",     category: "read",     arg: "url",     argLabel: "URL A · URL B",  argHint: "https://a.example  https://b.example",  short: "Compare two URLs", dual: true },
  { id: "brand",      name: "Brand",      tone: "cyan",   icon: "palette",  category: "read",     arg: "url",     argLabel: "URL",            argHint: "https://stripe.com",                   short: "Extract brand palette + type" },
  { id: "endpoints",  name: "Endpoints",  tone: "cyan",   icon: "plug",     category: "read",     arg: "url",     argLabel: "URL",            argHint: "https://app.example.com",              short: "Discover API endpoints" },

  /* ── Crawl & Ingest (orange · async lifecycle) ── */
  { id: "crawl",      name: "Crawl",      tone: "orange", icon: "crawl",    category: "jobs",     async: true, arg: "url",    argLabel: "start URL",   argHint: "https://docs.rs",                short: "Queue a recursive site crawl" },
  { id: "embed",      name: "Embed",      tone: "orange", icon: "layers",   category: "jobs",     async: true, arg: "input",  argLabel: "text or path", argHint: "./notes/*.md",                  short: "Embed text / files → vectors" },
  { id: "ingest",     name: "Ingest",     tone: "orange", icon: "box",      category: "jobs",     async: true, arg: "target", argLabel: "source",      argHint: "github.com/jmagar/axon",         short: "Ingest a repo / feed / sessions" },

  /* ── Search & Discover ── */
  { id: "search",     name: "Search",     tone: "cyan",   icon: "search",   category: "discover", arg: "query", argLabel: "query",          argHint: "rust async runtime comparison",     short: "Web search (Tavily / SearXNG)" },
  { id: "research",   name: "Research",   tone: "rose",   icon: "compass",  category: "discover", arg: "query", argLabel: "topic",          argHint: "state of WASM component model 2026", short: "Deep research + synthesis" },
  { id: "query",      name: "Query",      tone: "cyan",   icon: "target",   category: "discover", arg: "query", argLabel: "query",          argHint: "how does Pin work",                 short: "Vector search the collection" },
  { id: "sources",    name: "Sources",    tone: "cyan",   icon: "folder",   category: "discover", arg: null,    argLabel: "domain (optional)", argHint: "docs.rs",                       short: "List indexed sources", optionalArg: true, noArg: true },
  { id: "domains",    name: "Domains",    tone: "cyan",   icon: "globe",    category: "discover", arg: null,    argLabel: "filter (optional)", argHint: "rs",                            short: "List indexed domains", optionalArg: true, noArg: true },

  /* ── Reason (rose) ── */
  { id: "ask",        name: "Ask",        tone: "rose",   icon: "ask",      category: "reason",   arg: "query", argLabel: "question",       argHint: "how does serde derive work?",       short: "RAG over the collection" },

  /* ── System & Diagnostics (cyan) ── */
  { id: "doctor",     name: "Doctor",     tone: "cyan",   icon: "activity", category: "system",   arg: null, noArg: true, short: "Health-check the stack" },
  { id: "status",     name: "Status",     tone: "cyan",   icon: "server",   category: "system",   arg: null, noArg: true, short: "Server + job status" },
  { id: "stats",      name: "Stats",      tone: "cyan",   icon: "barChart", category: "system",   arg: null, noArg: true, short: "Collection statistics" },
];

const OP_BY_ID = Object.fromEntries(OPERATIONS.map((o) => [o.id, o]));
const OPS_BY_CATEGORY = CATEGORIES.map((c) => ({ ...c, ops: OPERATIONS.filter((o) => o.category === c.id) }));

/* Resolve an operation tone to its color trio. colorCode off → everything cyan. */
function toneOf(t, colorCode) {
  const c = colorCode ? t : "cyan";
  if (c === "orange") return { base: "var(--axon-orange)", fg: "var(--axon-orange-strong)", deep: "var(--axon-orange-deep)" };
  if (c === "rose")   return { base: "var(--aurora-accent-pink)", fg: "var(--aurora-accent-pink-strong)", deep: "var(--aurora-accent-pink-deep)" };
  return { base: "var(--aurora-accent-primary)", fg: "var(--aurora-accent-strong)", deep: "var(--aurora-accent-deep)" };
}

const tint = (color, pct, into = "transparent") => `color-mix(in srgb, ${color} ${pct}%, ${into})`;

function hostOf(u) {
  try { return String(u).replace(/^https?:\/\//, "").replace(/\/$/, ""); } catch { return u; }
}

/* Auto-detect an ingest source_type from a target URL or shorthand. */
function detectIngestSource(target) {
  const t = String(target || "").toLowerCase();
  if (/github\.com/.test(t)) return "github";
  if (/gitlab\.com/.test(t)) return "gitlab";
  if (/gitea|forgejo|codeberg\.org/.test(t)) return "gitea";
  if (/reddit\.com|^\/?r\//.test(t)) return "reddit";
  if (/youtube\.com|youtu\.be/.test(t)) return "youtube";
  if (/\.git($|\/)/.test(t) || /^https?:\/\//.test(t)) return "git";
  return "github";
}

window.AxonData = {
  CATEGORIES, OPERATIONS, OP_BY_ID, OPS_BY_CATEGORY,
  toneOf, tint, hostOf, detectIngestSource,
};
