import { defaultRehypePlugins, type ThemeInput } from "streamdown";

import { limitedCode } from "@/lib/limitedStreamdownCode";

// The plugin element type, derived from streamdown's own default map so we don't
// take a direct dependency on `unified`'s `Pluggable` type.
type RehypePlugin = (typeof defaultRehypePlugins)[string];

export const STREAMDOWN_PLUGINS = { code: limitedCode };
export const STREAMDOWN_CODE_THEMES: [ThemeInput, ThemeInput] = ["one-dark-pro", "one-dark-pro"];

// Hardened rehype pipeline for every <Streamdown> call site (S-M1/S-M2/S-L1).
//
// Streamdown's default `harden` config is fully permissive (`allowedImagePrefixes:
// ["*"]`, `allowDataImages: true`, `allowedProtocols: ["*"]`), so crawled/RAG
// content could embed tracking-beacon images or `data:`/non-http(s) links. Passing
// `rehypePlugins` to <Streamdown> REPLACES the default array, so we must re-include
// `raw` + `sanitize` (the XSS layer) before the locked-down `harden`:
//   - allowedImagePrefixes: [] — no external images at all (relative/asset: still work)
//   - allowDataImages: false   — no base64 image payloads
//   - allowedProtocols: ["http","https","mailto"] — drop data:/javascript:/file: etc.
//   - allowedLinkPrefixes: ["*"] — left permissive ON PURPOSE: the protocol
//     allowlist above is the real link guard (it already blocks javascript:/data:),
//     and crawled results legitimately link to arbitrary https hosts.
// This is defense-in-depth on top of the app CSP (`img-src 'self' asset: data:`),
// so a future CSP loosening does not silently re-expose the beacon/phishing surface.
export const STREAMDOWN_REHYPE_PLUGINS: RehypePlugin[] = [
  defaultRehypePlugins.raw,
  defaultRehypePlugins.sanitize,
  [
    (defaultRehypePlugins.harden as [unknown, Record<string, unknown>])[0],
    {
      allowedImagePrefixes: [],
      allowedLinkPrefixes: ["*"],
      allowedProtocols: ["http", "https", "mailto"],
      allowDataImages: false,
    },
  ] as unknown as RehypePlugin,
];
