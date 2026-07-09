// Pure URL-normalization helpers for the Browser palette tool. Split out from
// `BrowserView.tsx` so the address-bar/tab logic is unit-testable without
// mounting a component (see `apps/palette-tauri/CLAUDE.md` — prefer a
// `src/lib` helper with a co-located test over inlining logic in a
// component).
//
// UX intent mirrors the mock's `normUrl` at `palette-mock.html` (search
// `function browserView()`): a bare word/phrase with no dot and no scheme is
// treated as a search-engine query; anything host-shaped gets `https://`
// coercion; an already-schemed URL passes through unchanged.

/** Sentinel for the browser's home/new-tab page. Not a real network URL. */
export const BROWSER_HOME_URL = "about:blank";

const SEARCH_ENGINE_QUERY_URL = "https://duckduckgo.com/?q=";

/** True when `url` is the browser home/new-tab sentinel. */
export function isHomeUrl(url: string): boolean {
  return url.trim() === "" || url.trim() === BROWSER_HOME_URL || url.trim() === "home";
}

/**
 * True when `value` already looks like a navigable host or URL (has a
 * scheme, or is a dotted hostname/IP with no whitespace) rather than a
 * free-text search query.
 */
function looksNavigable(value: string): boolean {
  const trimmed = value.trim();
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed)) return true;
  if (/\s/.test(trimmed)) return false;
  // host[.tld][:port][/path] or bare "localhost[:port]"
  return /^localhost(:\d+)?(\/\S*)?$/i.test(trimmed) || /^[a-z0-9-]+(\.[a-z0-9-]+)+(:\d+)?(\/\S*)?$/i.test(trimmed);
}

/**
 * Normalize raw address-bar input into a navigable URL.
 *
 * - Empty input or the literal `"home"` → {@link BROWSER_HOME_URL}.
 * - Already-schemed URLs (`https://…`, `http://…`) pass through with only
 *   trailing-slash/whitespace trimming.
 * - Host-shaped input (`example.com`, `localhost:3000`, `127.0.0.1:8001`)
 *   gets `https://` prepended (`http://` for localhost/loopback, matching
 *   the palette's own `normalize_server_url` convention in `lib.rs`).
 * - Anything else (bare words, phrases with spaces, no dot) is treated as a
 *   search query and routed to the configured search engine.
 */
export function normalizeBrowserUrl(raw: string): string {
  const trimmed = raw.trim();
  if (isHomeUrl(trimmed)) return BROWSER_HOME_URL;

  if (/^https?:\/\//i.test(trimmed)) {
    return trimmed.replace(/\s+/g, "");
  }

  if (looksNavigable(trimmed)) {
    const isLoopback = /^(localhost|127\.0\.0\.1)(:\d+)?(\/\S*)?$/i.test(trimmed);
    return `${isLoopback ? "http://" : "https://"}${trimmed}`;
  }

  return `${SEARCH_ENGINE_QUERY_URL}${encodeURIComponent(trimmed)}`;
}

/** Human-friendly hostname for display in a tab label / favicon monogram. */
export function hostLabel(url: string): string {
  if (isHomeUrl(url)) return "New Tab";
  try {
    return new URL(url).hostname;
  } catch {
    return url.split("/")[0] || url;
  }
}

/** True when `url` is a search-engine query URL produced by this module. */
export function isSearchUrl(url: string): boolean {
  return url.startsWith(SEARCH_ENGINE_QUERY_URL);
}

/** Extract the original query text from a search URL produced by this module. */
export function searchQueryFrom(url: string): string {
  if (!isSearchUrl(url)) return "";
  try {
    return decodeURIComponent(url.slice(SEARCH_ENGINE_QUERY_URL.length));
  } catch {
    return "";
  }
}
