export type ArgMode = "none" | "optionalSingle" | "single" | "split";
type RemoteActionKind = "operation" | "job" | "admin" | "discovery";
type ActionTone = "info" | "success" | "warn" | "neutral" | "rose" | "orange";
export const JOB_FAMILIES = ["crawl", "embed", "extract", "ingest"] as const;
export type JobFamily = (typeof JOB_FAMILIES)[number];
export const JOB_OPERATIONS = ["list", "status", "cancel", "cleanup", "clear", "recover"] as const;
export type JobOperation = (typeof JOB_OPERATIONS)[number];

export type PaletteSubcommand =
  | "help"
  | "files"
  | "scrape"
  | "crawl"
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
  | "embed"
  | "extract"
  | "ingest"
  | "status"
  | "sources"
  | "domains"
  | "stats"
  | "doctor"
  | "endpoints"
  | "brand"
  | "diff"
  | "screenshot"
  | "dedupe"
  | "purge"
  | "watch-list"
  | "watch-create"
  | "watch-run"
  | "ingest-sessions-prepared"
  | "github"
  | `${JobFamily}-${JobOperation}`;

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
    label: "Files",
    subcommand: "files",
    kind: "local",
    argMode: "none",
    aliases: ["files", "browse-files", "filesystem", "explorer"],
    description: "Browse the local filesystem, preview or edit a file, and ingest it into the collection.",
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
    label: "Crawl URL",
    subcommand: "crawl",
    kind: "operation",
    argMode: "split",
    aliases: ["crawl", "site", "docs"],
    description: "Queue a recursive crawl from seed URLs. Runs async — returns a job you can tail, cancel, or recover.",
    example: "crawl https://docs.anthropic.com",
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
    description: "Search the web and enqueue crawls for useful results.",
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
    label: "Embed input",
    subcommand: "embed",
    kind: "operation",
    argMode: "single",
    aliases: ["embed", "index", "vectorize"],
    description: "Embed a URL, file, directory, or text input into the collection.",
    example: "embed https://docs.rs/serde",
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
    label: "Ingest target",
    subcommand: "ingest",
    kind: "operation",
    argMode: "split",
    aliases: ["ingest", "import", "repo", "youtube", "reddit"],
    description: "Ingest GitHub, Reddit, or YouTube targets into the collection.",
    example: "ingest https://github.com/zed-industries/zed",
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
    label: "Dedupe collection",
    subcommand: "dedupe",
    kind: "admin",
    argMode: "none",
    aliases: ["dedupe", "deduplicate", "clean-vectors"],
    description: "Remove near-duplicate chunks from the collection selected in palette settings.",
    example: "dedupe",
    tone: "warn",
  },
  {
    label: "Purge URL",
    subcommand: "purge",
    kind: "admin",
    argMode: "split",
    aliases: ["purge", "delete-url", "forget"],
    description: "Delete all indexed points for a URL from the collection. Destructive.",
    example: "purge https://docs.rs/serde",
    tone: "warn",
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
  {
    label: "Ingest prepared sessions",
    subcommand: "ingest-sessions-prepared",
    kind: "operation",
    argMode: "single",
    aliases: ["sessions-prepared", "ingest-sessions-prepared", "prepared-sessions"],
    description: "Submit a prepared AI-session ingest request as JSON.",
    example: "ingest-sessions-prepared {\"sessions\":[]}",
    tone: "orange",
  },
] as const satisfies readonly PaletteAction[];

type StaticSubcommand = Exclude<PaletteSubcommand, `${JobFamily}-${JobOperation}`>;
type ListedStaticSubcommand = (typeof STATIC_ACTIONS)[number]["subcommand"];
const _allStaticActionsListed: Exclude<StaticSubcommand, ListedStaticSubcommand> extends never ? true : never = true;
void _allStaticActionsListed;

export const ACTIONS: PaletteAction[] = [
  ...STATIC_ACTIONS,
  ...JOB_FAMILIES.flatMap(jobLifecycleActions),
];

function jobLifecycleActions(family: JobFamily): PaletteAction[] {
  const label = family[0].toUpperCase() + family.slice(1);
  return [
    {
      label: `List ${family} jobs`,
      subcommand: `${family}-list`,
      kind: "job",
      argMode: "none",
      aliases: [`${family}-list`, `${family}-jobs`, `${family}s`],
      description: `List recent ${family} jobs.`,
      example: `${family}-list`,
      tone: "neutral",
      autoRunOnSwitch: true,
    },
    {
      label: `${label} job status`,
      subcommand: `${family}-status`,
      kind: "job",
      argMode: "single",
      aliases: [`${family}-status`, `${family}-get`],
      description: `Fetch one ${family} job by UUID.`,
      example: `${family}-status 00000000-0000-4000-8000-000000000000`,
      tone: "info",
    },
    {
      label: `Cancel ${family} job`,
      subcommand: `${family}-cancel`,
      kind: "job",
      argMode: "single",
      aliases: [`${family}-cancel`, `cancel-${family}`],
      description: `Cancel one pending or running ${family} job by UUID.`,
      example: `${family}-cancel 00000000-0000-4000-8000-000000000000`,
      tone: "orange",
    },
    {
      label: `Cleanup ${family} jobs`,
      subcommand: `${family}-cleanup`,
      kind: "job",
      argMode: "none",
      aliases: [`${family}-cleanup`, `cleanup-${family}`],
      description: `Clean completed ${family} job records.`,
      example: `${family}-cleanup`,
      tone: "orange",
    },
    {
      label: `Clear ${family} jobs`,
      subcommand: `${family}-clear`,
      kind: "job",
      argMode: "none",
      aliases: [`${family}-clear`, `clear-${family}`],
      description: `Delete ${family} job records for a clean queue view.`,
      example: `${family}-clear`,
      tone: "warn",
    },
    {
      label: `Recover ${family} jobs`,
      subcommand: `${family}-recover`,
      kind: "job",
      argMode: "none",
      aliases: [`${family}-recover`, `recover-${family}`],
      description: `Recover stale ${family} jobs after an interrupted worker run.`,
      example: `${family}-recover`,
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
