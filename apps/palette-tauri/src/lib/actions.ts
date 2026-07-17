export type ArgMode = "none" | "optionalSingle" | "single" | "split";
type RemoteActionKind = "operation" | "job" | "admin" | "discovery";
type ActionTone = "info" | "success" | "warn" | "neutral" | "rose" | "orange";
export const JOB_OPERATIONS = ["list", "status", "cancel", "cleanup", "clear", "recover"] as const;
export type JobOperation = (typeof JOB_OPERATIONS)[number];
export type JobSubcommand = `jobs-${JobOperation}`;

export type PaletteSubcommand =
  | "help"
  | "browser"
  | "files"
  | "scrape"
  | "source-site"
  | "map"
  | "summarize"
  | "ask"
  | "chat"
  | "query"
  | "retrieve"
  | "suggest"
  | "evaluate"
  | "search"
  | "research"
  | "source"
  | "extract"
  | "status"
  | "sources"
  | "domains"
  | "stats"
  | "doctor"
  | "endpoints"
  | "brand"
  | "diff"
  | "screenshot"
  | "watch-list"
  | "watch-create"
  | "watch-run"
  | "github"
  | "terminal"
  | JobSubcommand;

interface PaletteActionBase {
  label: string;
  subcommand: PaletteSubcommand;
  argMode: ArgMode;
  aliases: string[];
  description: string;
  example: string;
  tone: ActionTone;
  autoRunOnSwitch?: boolean;
}

export interface LocalPaletteAction extends PaletteActionBase {
  kind: "local";
}

export interface RemotePaletteAction extends PaletteActionBase {
  kind: RemoteActionKind;
}

export type PaletteAction = LocalPaletteAction | RemotePaletteAction;

const STATIC_ACTIONS = [
  {
    label: "Help",
    subcommand: "help",
    kind: "local",
    argMode: "optionalSingle",
    aliases: ["help", "?", "--help", "-h"],
    description: "Show command help, usage, current request params, and available options.",
    example: "help scrape",
    tone: "info",
  },
  {
    label: "Browser",
    subcommand: "browser",
    kind: "local",
    argMode: "optionalSingle",
    aliases: ["browser", "web", "browse-url"],
    description: "Open a real in-app browser window — navigate to a URL or search the web.",
    example: "browser docs.rs/serde",
    tone: "info",
  },
  {
    label: "Files",
    subcommand: "files",
    kind: "local",
    argMode: "none",
    aliases: ["files", "browse-files", "filesystem", "explorer"],
    description: "Browse the local filesystem, preview or edit a file, and index it into the collection.",
    example: "files",
    tone: "orange",
  },
  {
    label: "Scrape URL",
    subcommand: "scrape",
    kind: "operation",
    argMode: "split",
    aliases: ["scrape", "fetch", "page", "url"],
    description: "Fetch a single page, convert it to clean markdown, and optionally embed it into the collection.",
    example: "scrape https://docs.rs/serde",
    tone: "info",
  },
  {
    label: "Index site",
    subcommand: "source-site",
    kind: "operation",
    argMode: "split",
    aliases: ["site", "index-site", "docs"],
    description: "Index a site through the unified source pipeline. Returns a job you can inspect or cancel.",
    example: "source-site https://docs.anthropic.com",
    tone: "warn",
  },
  {
    label: "Map URL",
    subcommand: "map",
    kind: "operation",
    argMode: "split",
    aliases: ["map", "links", "discover"],
    description: "Walk a domain and return the URL graph without fetching page bodies. Fast reconnaissance.",
    example: "map https://code.claude.com/docs",
    tone: "info",
  },
  {
    label: "Summarize URL",
    subcommand: "summarize",
    kind: "operation",
    argMode: "split",
    aliases: ["summarize", "summary", "brief"],
    description: "Scrape one or more URLs and synthesize a concise summary.",
    example: "summarize https://docs.rs/serde",
    tone: "rose",
  },
  {
    label: "Ask question",
    subcommand: "ask",
    kind: "operation",
    argMode: "single",
    aliases: ["ask", "answer", "rag"],
    description: "Run RAG over the configured collection and synthesize an answer.",
    example: "ask how does the Axon palette talk to REST?",
    tone: "rose",
  },
  {
    label: "Chat with LLM",
    subcommand: "chat",
    kind: "operation",
    argMode: "single",
    aliases: ["chat", "llm", "talk"],
    description: "Chat directly with the configured LLM without RAG retrieval.",
    example: "chat explain this error simply",
    tone: "rose",
  },
  {
    label: "Query knowledge base",
    subcommand: "query",
    kind: "operation",
    argMode: "single",
    aliases: ["query", "vector", "semantic"],
    description: "Search indexed chunks semantically and return ranked source snippets.",
    example: "query tauri palette openapi",
    tone: "neutral",
  },
  {
    label: "Retrieve document",
    subcommand: "retrieve",
    kind: "operation",
    argMode: "split",
    aliases: ["retrieve", "chunks", "document"],
    description: "Return the embedded chunks already stored for a URL, paged. Inline-first reading action.",
    example: "retrieve https://docs.rs/serde",
    tone: "neutral",
  },
  {
    label: "Suggest URLs",
    subcommand: "suggest",
    kind: "operation",
    argMode: "optionalSingle",
    aliases: ["suggest", "recommend", "discover-more"],
    description: "Suggest additional documentation URLs worth crawling.",
    example: "suggest tauri",
    tone: "rose",
  },
  {
    label: "Evaluate answer",
    subcommand: "evaluate",
    kind: "operation",
    argMode: "single",
    aliases: ["evaluate", "eval", "judge"],
    description: "Compare RAG and baseline answers with an independent LLM judge.",
    example: "evaluate how does Tauri v2 HTTP work?",
    tone: "rose",
  },
  {
    label: "Search the web",
    subcommand: "search",
    kind: "operation",
    argMode: "single",
    aliases: ["search", "web"],
    description: "Search the web and index useful results.",
    example: "search tauri v2 openapi client",
    tone: "info",
  },
  {
    label: "Research the web",
    subcommand: "research",
    kind: "operation",
    argMode: "single",
    aliases: ["research", "deepsearch"],
    description: "Run web research with LLM synthesis.",
    example: "research qdrant hybrid search tuning",
    tone: "rose",
  },
  {
    label: "Index source",
    subcommand: "source",
    kind: "operation",
    argMode: "single",
    aliases: ["source", "index", "add-source"],
    description: "Index a URL, repository, feed, session selector, file, or directory through the unified source pipeline.",
    example: "source https://docs.rs/serde",
    tone: "orange",
  },
  {
    label: "Extract data",
    subcommand: "extract",
    kind: "operation",
    argMode: "split",
    aliases: ["extract", "structured", "parse"],
    description: "Queue structured extraction for one or more URLs.",
    example: "extract https://example.com/pricing",
    tone: "orange",
  },
  {
    label: "Browse GitHub",
    subcommand: "github",
    kind: "operation",
    argMode: "single",
    aliases: ["github", "gh", "repo-browse"],
    description: "Browse a GitHub owner's repos, a repo's file tree, or preview a file's contents.",
    example: "github jmagar/axon/README.md",
    tone: "neutral",
  },
  {
    label: "Job status",
    subcommand: "status",
    kind: "discovery",
    argMode: "none",
    aliases: ["status", "jobs", "queue"],
    description: "Show the async job queue and recent worker state.",
    example: "status",
    tone: "neutral",
    autoRunOnSwitch: true,
  },
  {
    label: "List sources",
    subcommand: "sources",
    kind: "discovery",
    argMode: "none",
    aliases: ["sources", "urls", "indexed"],
    description: "List indexed source URLs in the configured collection.",
    example: "sources",
    tone: "neutral",
    autoRunOnSwitch: true,
  },
  {
    label: "List domains",
    subcommand: "domains",
    kind: "discovery",
    argMode: "none",
    aliases: ["domains", "sites", "facets"],
    description: "Show indexed domains and vector counts.",
    example: "domains",
    tone: "neutral",
    autoRunOnSwitch: true,
  },
  {
    label: "Collection stats",
    subcommand: "stats",
    kind: "discovery",
    argMode: "none",
    aliases: ["stats", "collection", "qdrant"],
    description: "Show vector collection statistics.",
    example: "stats",
    tone: "neutral",
    autoRunOnSwitch: true,
  },
  {
    label: "Doctor",
    subcommand: "doctor",
    kind: "discovery",
    argMode: "none",
    aliases: ["doctor", "health", "check"],
    description: "Check Qdrant, TEI, and LLM connectivity.",
    example: "doctor",
    tone: "info",
    autoRunOnSwitch: true,
  },
  {
    label: "Discover endpoints",
    subcommand: "endpoints",
    kind: "operation",
    argMode: "split",
    aliases: ["endpoints", "api", "routes", "discover-endpoints"],
    description: "Scan a page for API endpoints, scripts, and optional probes.",
    example: "endpoints https://example.com/app",
    tone: "info",
  },
  {
    label: "Extract brand",
    subcommand: "brand",
    kind: "operation",
    argMode: "split",
    aliases: ["brand", "identity", "colors", "logo", "favicon"],
    description: "Extract brand name, colors, fonts, and logo assets from a URL.",
    example: "brand https://aurora.tootie.tv",
    tone: "rose",
  },
  {
    label: "Diff URLs",
    subcommand: "diff",
    kind: "operation",
    argMode: "split",
    aliases: ["diff", "compare", "changes"],
    description: "Render two URLs and diff their extracted content — track what changed between versions.",
    example: "diff https://example.com/a https://example.com/b",
    tone: "info",
  },
  {
    label: "Screenshot URL",
    subcommand: "screenshot",
    kind: "operation",
    argMode: "split",
    aliases: ["screenshot", "capture", "shot", "png"],
    description: "Render a URL in Chrome and capture a full-page screenshot using the default viewport.",
    example: "screenshot https://example.com",
    tone: "info",
  },
  {
    label: "List watches",
    subcommand: "watch-list",
    kind: "admin",
    argMode: "none",
    aliases: ["watch", "watches", "watch-list"],
    description: "List scheduled URL change-detection watches.",
    example: "watch-list",
    tone: "neutral",
    autoRunOnSwitch: true,
  },
  {
    label: "Create URL watch",
    subcommand: "watch-create",
    kind: "admin",
    argMode: "split",
    aliases: ["watch-create", "watch-url", "monitor-url"],
    description: "Create a URL change detector. Optional second argument is seconds.",
    example: "watch-create https://example.com/docs 3600",
    tone: "orange",
  },
  {
    label: "Run watch now",
    subcommand: "watch-run",
    kind: "admin",
    argMode: "single",
    aliases: ["watch-run", "run-watch", "watch-now"],
    description: "Run an existing watch definition immediately by UUID.",
    example: "watch-run 00000000-0000-4000-8000-000000000000",
    tone: "info",
  },
  { label: "Terminal", subcommand: "terminal", kind: "local", argMode: "none", aliases: ["terminal", "shell", "sh", "console", "cmd"], description: "Run real shell commands in a persistent session with your actual working directory. Desktop app only.", example: "terminal", tone: "neutral", autoRunOnSwitch: true },
] as const satisfies readonly PaletteAction[];

type StaticSubcommand = Exclude<PaletteSubcommand, JobSubcommand>;
type ListedStaticSubcommand = (typeof STATIC_ACTIONS)[number]["subcommand"];
const _allStaticActionsListed: Exclude<StaticSubcommand, ListedStaticSubcommand> extends never ? true : never = true;
void _allStaticActionsListed;

export const ACTIONS: PaletteAction[] = [
  ...STATIC_ACTIONS,
  ...jobLifecycleActions(),
];

function jobLifecycleActions(): PaletteAction[] {
  return [
    {
      label: "List jobs",
      subcommand: "jobs-list",
      kind: "job",
      argMode: "none",
      aliases: ["jobs-list", "job-list", "queue-list"],
      description: "List recent jobs across the unified job store.",
      example: "jobs-list",
      tone: "neutral",
      autoRunOnSwitch: true,
    },
    {
      label: "Job status",
      subcommand: "jobs-status",
      kind: "job",
      argMode: "single",
      aliases: ["jobs-status", "job-status", "job-get"],
      description: "Fetch one job by UUID.",
      example: "jobs-status 00000000-0000-4000-8000-000000000000",
      tone: "info",
    },
    {
      label: "Cancel job",
      subcommand: "jobs-cancel",
      kind: "job",
      argMode: "single",
      aliases: ["jobs-cancel", "job-cancel", "cancel-job"],
      description: "Cancel one pending or running job by UUID.",
      example: "jobs-cancel 00000000-0000-4000-8000-000000000000",
      tone: "orange",
    },
    {
      label: "Cleanup jobs",
      subcommand: "jobs-cleanup",
      kind: "job",
      argMode: "none",
      aliases: ["jobs-cleanup", "job-cleanup", "cleanup-jobs"],
      description: "Clean completed job records.",
      example: "jobs-cleanup",
      tone: "orange",
    },
    {
      label: "Clear jobs",
      subcommand: "jobs-clear",
      kind: "job",
      argMode: "none",
      aliases: ["jobs-clear", "job-clear", "clear-jobs"],
      description: "Delete job records for a clean queue view.",
      example: "jobs-clear",
      tone: "warn",
    },
    {
      label: "Recover jobs",
      subcommand: "jobs-recover",
      kind: "job",
      argMode: "none",
      aliases: ["jobs-recover", "job-recover", "recover-jobs"],
      description: "Recover stale jobs after an interrupted worker run.",
      example: "jobs-recover",
      tone: "success",
    },
  ];
}

export {
  actionMatches,
  actionInvokedBy,
  acceptsDirectUrl,
  coercesArgumentToUrl,
} from "./actionPredicates";
