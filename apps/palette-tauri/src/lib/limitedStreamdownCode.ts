import type { TokensResult } from "shiki";
import type { BundledLanguage, CodeHighlighterPlugin, HighlightOptions, ThemeInput } from "streamdown";
import { createHighlighterCore } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import bash from "shiki/langs/bash.mjs";
import json from "shiki/langs/json.mjs";
import markdown from "shiki/langs/markdown.mjs";
import python from "shiki/langs/python.mjs";
import rust from "shiki/langs/rust.mjs";
import toml from "shiki/langs/toml.mjs";
import typescript from "shiki/langs/typescript.mjs";
import yaml from "shiki/langs/yaml.mjs";
import oneDarkPro from "shiki/themes/one-dark-pro.mjs";

const SUPPORTED_LANGUAGES = ["rust", "json", "bash", "toml", "yaml", "markdown", "typescript", "python"] as const;
const THEMES: [ThemeInput, ThemeInput] = ["one-dark-pro", "one-dark-pro"];

type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number];

const LANGUAGE_ALIASES: Record<string, SupportedLanguage> = {
  rs: "rust",
  shell: "bash",
  shellscript: "bash",
  sh: "bash",
  zsh: "bash",
  yml: "yaml",
  md: "markdown",
  ts: "typescript",
  py: "python",
};

// Lazy: the shiki core (8 grammars + theme + regex engine) is heavy and was
// previously instantiated at module eval, on the startup critical path (P-H1).
// A fresh palette launch shows only the command bar + action list, so defer the
// highlighter build until the first code block actually needs highlighting.
type Highlighter = Awaited<ReturnType<typeof createHighlighterCore>>;
let highlighterPromise: Promise<Highlighter> | undefined;

function getHighlighter(): Promise<Highlighter> {
  highlighterPromise ??= createHighlighterCore({
    themes: [oneDarkPro],
    langs: [rust, json, bash, toml, yaml, markdown, typescript, python].flat(),
    engine: createJavaScriptRegexEngine({ forgiving: true }),
  });
  return highlighterPromise;
}

const highlighted = new Map<string, TokensResult>();

export const limitedCode: CodeHighlighterPlugin = {
  name: "shiki",
  type: "code-highlighter",
  supportsLanguage(language) {
    return normalizeLanguage(language) !== undefined;
  },
  getSupportedLanguages() {
    return [...SUPPORTED_LANGUAGES] as BundledLanguage[];
  },
  getThemes() {
    return THEMES;
  },
  highlight(options, callback) {
    const language = normalizeLanguage(options.language);
    if (!language) return null;

    const key = cacheKey(options, language);
    const cached = highlighted.get(key);
    if (cached) return cached;

    getHighlighter()
      .then((highlighter) => {
        const result = highlighter.codeToTokens(options.code, {
          lang: language,
          themes: {
            light: themeName(options.themes[0]),
            dark: themeName(options.themes[1]),
          },
        }) as TokensResult;
        highlighted.set(key, result);
        callback?.(result);
      })
      .catch((error: unknown) => {
        console.error("[Axon Palette] Failed to highlight code block", error);
      });

    return null;
  },
};

function normalizeLanguage(language: string): SupportedLanguage | undefined {
  const normalized = language.trim().toLowerCase();
  if ((SUPPORTED_LANGUAGES as readonly string[]).includes(normalized)) return normalized as SupportedLanguage;
  return LANGUAGE_ALIASES[normalized];
}

function themeName(theme: ThemeInput): string {
  return typeof theme === "string" ? theme : (theme.name ?? "one-dark-pro");
}

function cacheKey(options: HighlightOptions, language: SupportedLanguage): string {
  const head = options.code.slice(0, 80);
  const tail = options.code.length > 80 ? options.code.slice(-80) : "";
  return `${language}:${themeName(options.themes[0])}:${themeName(options.themes[1])}:${options.code.length}:${head}:${tail}`;
}
