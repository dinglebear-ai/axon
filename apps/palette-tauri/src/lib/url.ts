// Canonical URL helpers. Single home for host-label and first-URL extraction so
// the command bar, footer, and result views render URLs identically.

/**
 * Human-friendly host label for a URL. Uses `new URL(url).hostname`, which
 * strips any `:port` (so `https://example.com:8080/x` → `example.com`). Falls
 * back to the leading path segment, then the raw input, when the URL cannot be
 * parsed.
 */
export function hostLabel(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return url.split("/")[0] || url;
  }
}

/** First `http(s)` URL found in `text`, or `null` when there is none. */
export function firstUrl(text: string): string | null {
  return text.match(/https?:\/\/[^\s"')\]}]+/i)?.[0] ?? null;
}
