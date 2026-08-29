import type { TokensResult } from "shiki";
import type { BundledLanguage, CodeHighlighterPlugin, HighlightOptions, ThemeInput } from "streamdown";
import { createHighlighterCore } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import bash from "shiki/langs/bash.mjs";
import c from "shiki/langs/c.mjs";
import cpp from "shiki/langs/cpp.mjs";
import css from "shiki/langs/css.mjs";
import diff from "shiki/langs/diff.mjs";
import dockerfile from "shiki/langs/dockerfile.mjs";
import go from "shiki/langs/go.mjs";
import html from "shiki/langs/html.mjs";
import ini from "shiki/langs/ini.mjs";
import java from "shiki/langs/java.mjs";
import javascript from "shiki/langs/javascript.mjs";
import jsx from "shiki/langs/jsx.mjs";
import json from "shiki/langs/json.mjs";
import markdown from "shiki/langs/markdown.mjs";
import php from "shiki/langs/php.mjs";
import python from "shiki/langs/python.mjs";
import ruby from "shiki/langs/ruby.mjs";
import rust from "shiki/langs/rust.mjs";
import sql from "shiki/langs/sql.mjs";
import svelte from "shiki/langs/svelte.mjs";
import toml from "shiki/langs/toml.mjs";
import tsx from "shiki/langs/tsx.mjs";
import typescript from "shiki/langs/typescript.mjs";
import vue from "shiki/langs/vue.mjs";
import xml from "shiki/langs/xml.mjs";
import yaml from "shiki/langs/yaml.mjs";
import oneDarkPro from "shiki/themes/one-dark-pro.mjs";

const SUPPORTED_LANGUAGES = [
  "rust",
  "json",
  "bash",
  "toml",
  "yaml",
  "markdown",
  "typescript",
  "tsx",
  "javascript",
  "jsx",
  "html",
  "css",
  "python",
  "sql",
  "c",
  "cpp",
  "go",
  "java",
  "ruby",
  "php",
  "xml",
  "vue",
  "svelte",
  "dockerfile",
  "ini",
  "diff",
] as const;
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
  mts: "typescript",
  cts: "typescript",
  js: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  htm: "html",
  xhtml: "html",
  svg: "xml",
  plist: "xml",
  rb: "ruby",
  gemfile: "ruby",
  h: "c",
  cc: "cpp",
  cxx: "cpp",
  hpp: "cpp",
  properties: "ini",
  conf: "ini",
  patch: "diff",
  py: "python",
};

// Lazy: the Shiki core, grammars, theme, and regex engine are heavy and were
// previously instantiated at module eval, on the startup critical path (P-H1).
// A fresh palette launch shows only the command bar + action list, so defer the
// highlighter build until the first code block actually needs highlighting.
type Highlighter = Awaited<ReturnType<typeof createHighlighterCore>>;
let highlighterPromise: Promise<Highlighter> | undefined;

function getHighlighter(): Promise<Highlighter> {
  highlighterPromise ??= createHighlighterCore({
    themes: [oneDarkPro],
    langs: [
      rust,
      json,
      bash,
      toml,
      yaml,
      markdown,
      typescript,
      tsx,
      javascript,
      jsx,
      html,
      css,
      python,
      sql,
      c,
      cpp,
      go,
      java,
      ruby,
      php,
      xml,
      vue,
      svelte,
      dockerfile,
      ini,
      diff,
    ].flat(),
    engine: createJavaScriptRegexEngine({ forgiving: true }),
  });
  return highlighterPromise;
}

/** Highlight a complete local file preview with the same constrained grammar
 * set and theme used by Markdown code blocks. Shiki escapes source text before
 * producing HTML, so callers can safely mount the returned markup. */
export async function highlightCodeHtml(code: string, language: string): Promise<string | null> {
  const normalized = normalizeLanguage(language);
  if (!normalized) return null;
  const highlighter = await getHighlighter();
  return highlighter.codeToHtml(code, { lang: normalized, theme: "one-dark-pro" });
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
