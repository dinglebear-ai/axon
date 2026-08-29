export interface FileSymbol {
  label: string;
  kind: "heading" | "function" | "class" | "selector" | "markup";
  line: number;
}

const SYMBOL_PATTERNS: Array<[FileSymbol["kind"], RegExp]> = [
  ["heading", /^\s{0,3}#{1,6}\s+(.+)$/],
  ["function", /^\s*(?:export\s+)?(?:async\s+)?(?:function|fn|def)\s+([\w$]+)/],
  ["function", /^\s*(?:export\s+)?(?:const|let|var)\s+([\w$]+)\s*=\s*(?:async\s*)?\(/],
  ["class", /^\s*(?:export\s+)?(?:class|struct|enum|interface|trait)\s+([\w$]+)/],
  ["markup", /^\s*<(?:section|main|article|nav|header|footer)\b[^>]*(?:id|class)=["']([^"']+)/i],
  ["selector", /^\s*([.#][\w-]+(?:\s+[.#]?[\w-]+)*)\s*\{/],
];

export function extractFileSymbols(content: string, limit = 120): FileSymbol[] {
  const symbols: FileSymbol[] = [];
  for (const [index, line] of content.split("\n").entries()) {
    for (const [kind, pattern] of SYMBOL_PATTERNS) {
      const match = pattern.exec(line);
      if (!match?.[1]) continue;
      symbols.push({ label: match[1].trim(), kind, line: index + 1 });
      break;
    }
    if (symbols.length >= limit) break;
  }
  return symbols;
}

export function countTextMatches(content: string, query: string): number {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return 0;
  let count = 0;
  let cursor = 0;
  const haystack = content.toLocaleLowerCase();
  let matchIndex = haystack.indexOf(needle, cursor);
  while (matchIndex !== -1) {
    count += 1;
    cursor = matchIndex + Math.max(needle.length, 1);
    matchIndex = haystack.indexOf(needle, cursor);
  }
  return count;
}

export function languageForFile(name: string): string {
  const lower = name.toLowerCase();
  if (lower === "dockerfile") return "dockerfile";
  if (["gemfile", "rakefile"].includes(lower)) return "ruby";
  const extension = lower.includes(".") ? (lower.split(".").pop() ?? "text") : "text";
  const aliases: Record<string, string> = {
    htm: "html",
    mjs: "javascript",
    cjs: "javascript",
    js: "javascript",
    mts: "typescript",
    cts: "typescript",
    ts: "typescript",
    md: "markdown",
    mdx: "markdown",
    yml: "yaml",
    sh: "bash",
    zsh: "bash",
    svg: "xml",
    rb: "ruby",
    cc: "cpp",
    cxx: "cpp",
    hpp: "cpp",
    patch: "diff",
  };
  return aliases[extension] ?? extension;
}

export function selectionPrompt(action: "ask" | "explain" | "improve", selection: string): string {
  const prefix =
    action === "ask"
      ? "Answer a question about this selection"
      : action === "explain"
        ? "Explain this selection clearly"
        : "Improve this selection while preserving its behavior";
  return `${prefix}:\n\n${selection}`;
}
