export type ArgMode = "none" | "optionalSingle" | "single" | "split";

export interface PaletteAction {
  label: string;
  subcommand: string;
  argMode: ArgMode;
  aliases: string[];
  description: string;
  example: string;
  tone: "info" | "success" | "warn" | "neutral" | "rose" | "violet";
}

export const ACTIONS: PaletteAction[] = [
  {
    label: "Scrape URL",
    subcommand: "scrape",
    argMode: "split",
    aliases: ["scrape", "fetch", "page", "url"],
    description: "Fetch one page, convert it to markdown, and optionally embed it.",
    example: "scrape https://docs.rs/serde",
    tone: "info",
  },
  {
    label: "Crawl URL",
    subcommand: "crawl",
    argMode: "split",
    aliases: ["crawl", "site", "docs"],
    description: "Queue a site crawl from a start URL with the current crawl settings.",
    example: "crawl https://docs.anthropic.com",
    tone: "violet",
  },
  {
    label: "Map URL",
    subcommand: "map",
    argMode: "split",
    aliases: ["map", "links", "discover"],
    description: "Discover URLs without scraping or embedding page content.",
    example: "map https://code.claude.com/docs",
    tone: "info",
  },
  {
    label: "Summarize URL",
    subcommand: "summarize",
    argMode: "split",
    aliases: ["summarize", "summary", "brief"],
    description: "Scrape one or more URLs and synthesize a concise summary.",
    example: "summarize https://docs.rs/serde",
    tone: "rose",
  },
  {
    label: "Ask question",
    subcommand: "ask",
    argMode: "single",
    aliases: ["ask", "answer", "rag"],
    description: "Run RAG over the configured collection and synthesize an answer.",
    example: "ask how does the Axon palette talk to REST?",
    tone: "rose",
  },
  {
    label: "Query knowledge base",
    subcommand: "query",
    argMode: "single",
    aliases: ["query", "vector", "semantic"],
    description: "Search indexed chunks semantically and return ranked source snippets.",
    example: "query tauri palette openapi",
    tone: "neutral",
  },
  {
    label: "Retrieve document",
    subcommand: "retrieve",
    argMode: "split",
    aliases: ["retrieve", "chunks", "document"],
    description: "Fetch stored document content for an indexed URL.",
    example: "retrieve https://docs.rs/serde",
    tone: "neutral",
  },
  {
    label: "Suggest URLs",
    subcommand: "suggest",
    argMode: "optionalSingle",
    aliases: ["suggest", "recommend", "discover-more"],
    description: "Suggest additional documentation URLs worth crawling.",
    example: "suggest tauri",
    tone: "violet",
  },
  {
    label: "Evaluate answer",
    subcommand: "evaluate",
    argMode: "single",
    aliases: ["evaluate", "eval", "judge"],
    description: "Compare RAG and baseline answers with an independent LLM judge.",
    example: "evaluate how does Tauri v2 HTTP work?",
    tone: "violet",
  },
  {
    label: "Search the web",
    subcommand: "search",
    argMode: "single",
    aliases: ["search", "web"],
    description: "Search the web and enqueue crawls for useful results.",
    example: "search tauri v2 openapi client",
    tone: "info",
  },
  {
    label: "Research the web",
    subcommand: "research",
    argMode: "single",
    aliases: ["research", "deepsearch"],
    description: "Run web research with LLM synthesis.",
    example: "research qdrant hybrid search tuning",
    tone: "rose",
  },
  {
    label: "Embed input",
    subcommand: "embed",
    argMode: "single",
    aliases: ["embed", "index", "vectorize"],
    description: "Embed a URL, file, directory, or text input into the collection.",
    example: "embed https://docs.rs/serde",
    tone: "violet",
  },
  {
    label: "Extract data",
    subcommand: "extract",
    argMode: "split",
    aliases: ["extract", "structured", "parse"],
    description: "Queue structured extraction for one or more URLs.",
    example: "extract https://example.com/pricing",
    tone: "violet",
  },
  {
    label: "Ingest target",
    subcommand: "ingest",
    argMode: "split",
    aliases: ["ingest", "import", "repo", "youtube", "reddit"],
    description: "Ingest GitHub, Reddit, or YouTube targets into the collection.",
    example: "ingest https://github.com/zed-industries/zed",
    tone: "warn",
  },
  {
    label: "Job status",
    subcommand: "status",
    argMode: "none",
    aliases: ["status", "jobs", "queue"],
    description: "Show the async job queue and recent worker state.",
    example: "status",
    tone: "neutral",
  },
  {
    label: "List sources",
    subcommand: "sources",
    argMode: "none",
    aliases: ["sources", "urls", "indexed"],
    description: "List indexed source URLs in the configured collection.",
    example: "sources",
    tone: "neutral",
  },
  {
    label: "List domains",
    subcommand: "domains",
    argMode: "none",
    aliases: ["domains", "sites", "facets"],
    description: "Show indexed domains and vector counts.",
    example: "domains",
    tone: "neutral",
  },
  {
    label: "Collection stats",
    subcommand: "stats",
    argMode: "none",
    aliases: ["stats", "collection", "qdrant"],
    description: "Show vector collection statistics.",
    example: "stats",
    tone: "neutral",
  },
  {
    label: "Doctor",
    subcommand: "doctor",
    argMode: "none",
    aliases: ["doctor", "health", "check"],
    description: "Check Qdrant, TEI, and LLM connectivity.",
    example: "doctor",
    tone: "info",
  },
];

export function actionMatches(action: PaletteAction, input: string): boolean {
  const query = input.trim().toLowerCase();
  if (!query) return true;
  return [
    action.label,
    action.subcommand,
    action.description,
    action.example,
    ...action.aliases,
  ].some((value) => value.toLowerCase().includes(query));
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
  return ["scrape", "crawl", "map", "summarize", "retrieve", "embed", "extract", "ingest"].includes(
    action.subcommand,
  );
}
