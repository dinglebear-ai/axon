import type { PaletteAction, PaletteSubcommand } from "./actions";

export function actionMatches(action: PaletteAction, input: string): boolean {
  const query = input.trim().toLowerCase();
  if (!query) return true;
  return [action.label, action.subcommand, ...action.aliases].some((value) =>
    value.toLowerCase().includes(query),
  );
}

export function actionInvokedBy(action: PaletteAction, token: string): boolean {
  const normalized = token.trim().toLowerCase();
  return (
    normalized.length > 0 &&
    (action.subcommand.toLowerCase() === normalized ||
      action.aliases.some((alias) => alias.toLowerCase() === normalized))
  );
}

export function acceptsDirectUrl(action: PaletteAction): boolean {
  return [
    "scrape",
    "crawl",
    "map",
    "summarize",
    "retrieve",
    "embed",
    "extract",
    "ingest",
    "endpoints",
    "brand",
    "screenshot",
    "watch-create",
    "purge",
  ].includes(action.subcommand);
}

// Actions whose argument may be a NON-URL target, so a scheme-less argument must
// NOT be coerced into an `https://` URL. They still accept a URL — it's just
// passed through verbatim and classified server-side:
//   - `ingest`: `owner/repo`, `r/subreddit`, a YouTube id, … (coercing
//     `ingest unraid/api` → `https://unraid/api` breaks `parse_github_repo`).
//   - `embed`: a file, a directory, or free text (coercing `embed some notes`
//     → `https://some notes` corrupts the input).
const BARE_TARGET_SUBCOMMANDS = new Set<PaletteSubcommand>(["ingest", "embed"]);

/**
 * Return whether a bare scheme-less argument should be coerced to an HTTPS URL.
 *
 * @param action - Palette action being submitted.
 * @returns True when a direct URL action should receive `https://` coercion.
 */
export function coercesArgumentToUrl(action: PaletteAction): boolean {
  return acceptsDirectUrl(action) && !BARE_TARGET_SUBCOMMANDS.has(action.subcommand);
}
