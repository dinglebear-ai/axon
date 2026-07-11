// Client-side pre-redaction + blocked-capture-scheme guard, shared by the
// service worker (background.js, via importScripts) and the popup
// (popup.html <script> tag, loaded before popup-actions.js).
//
// chrome-extension-contract.md "Capture Contract": "page snapshots are
// redacted before upload when client-side detectors can do so and always
// redacted again server-side"; "Security Contract": "must not ... collect
// cookies, auth headers, or local storage". This is a best-effort first
// pass, not the authoritative redaction layer — the server always
// re-redacts.
//
// EVERY capture payload that leaves the browser (selection text, page
// snapshot content, memory bodies, link hrefs) must be routed through
// `AxonRedact.redactText`/`AxonRedact.redactUrl` before it reaches
// `postAxon`/`getAxon`. Blocked-scheme captures must be checked with
// `AxonRedact.isBlockedCaptureUrl`/`blockedCaptureReason` before any
// chrome.tabs/chrome.scripting call touches the page.
(function (global) {
  "use strict";

  const PATTERNS = [
    // Authorization / Bearer / API-key-shaped tokens.
    { name: "bearer_token", re: /\b(bearer\s+)[A-Za-z0-9._-]{16,}/gi, replace: (_m, prefix) => `${prefix}[REDACTED]` },
    // `Authorization: <scheme> <value>` header lines (any scheme, not just Bearer).
    { name: "auth_header", re: /\b(authorization\s*:\s*)\S+.*$/gim, replace: (_m, prefix) => `${prefix}[REDACTED]` },
    // `Cookie:`/`Set-Cookie:` header lines and values.
    { name: "cookie_header", re: /\b((?:set-)?cookie\s*:\s*).+$/gim, replace: (_m, prefix) => `${prefix}[REDACTED]` },
    // Credentials embedded in a URL (https://user:pass@host/...).
    { name: "url_credentials", re: /\b([a-z][a-z0-9+.-]*:\/\/)[^\s/@]+:[^\s/@]+@/gi, replace: (_m, scheme) => `${scheme}[REDACTED]@` },
    // Common `key=value` / `key: value` secret-ish fields.
    { name: "secret_kv", re: /\b((?:api|secret|access|private|client)[_-]?(?:key|token|secret))\s*[:=]\s*["']?[A-Za-z0-9._-]{8,}["']?/gi, replace: (_m, label) => `${label}=[REDACTED]` },
    // AWS-shaped access key IDs.
    { name: "aws_access_key", re: /\b(AKIA|ASIA)[A-Z0-9]{16}\b/g, replace: () => "[REDACTED_AWS_KEY]" },
    // JWT-shaped three-part base64url tokens.
    { name: "jwt", re: /\beyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\b/g, replace: () => "[REDACTED_JWT]" },
    // Generic long hex/base64-shaped secret blobs (32+ chars, no whitespace).
    { name: "opaque_secret_blob", re: /\b[A-Za-z0-9_-]{40,}\b/g, replace: (m) => (/[0-9]/.test(m) && /[A-Za-z]/.test(m) ? "[REDACTED_TOKEN]" : m) }
  ];

  // Browser-internal / privileged schemes that must never be captured
  // (page content, selection, screenshot, or auto-scrape).
  const BLOCKED_SCHEME_RE = /^(chrome|chrome-extension|edge|about|devtools|view-source|file|data|blob):/i;

  function redactText(text) {
    let output = String(text || "");
    const redactions = [];
    for (const pattern of PATTERNS) {
      pattern.re.lastIndex = 0;
      if (pattern.re.test(output)) {
        redactions.push(pattern.name);
      }
      pattern.re.lastIndex = 0;
      output = output.replace(pattern.re, pattern.replace);
    }
    return { text: output, redactions };
  }

  // Strips embedded basic-auth credentials from a URL. Query-string
  // secrets are left to redactText since href values otherwise need to
  // stay navigable.
  function redactUrl(url) {
    const value = String(url || "");
    try {
      const parsed = new URL(value);
      if (parsed.username || parsed.password) {
        const scheme = `${parsed.protocol}//`;
        parsed.username = "";
        parsed.password = "";
        const withoutCredentials = parsed.toString();
        const rest = withoutCredentials.startsWith(scheme) ? withoutCredentials.slice(scheme.length) : withoutCredentials;
        return `${scheme}[REDACTED]@${rest}`;
      }
      return value;
    } catch {
      return redactText(value).text;
    }
  }

  function isBlockedCaptureUrl(url) {
    return BLOCKED_SCHEME_RE.test(String(url || "").trim());
  }

  function blockedCaptureReason(url) {
    if (!isBlockedCaptureUrl(url)) {
      return null;
    }
    const scheme = (String(url || "").match(BLOCKED_SCHEME_RE) || [])[1] || "this";
    return `Axon can't capture ${scheme}: pages — only http:// and https:// tabs can be scraped, crawled, or remembered.`;
  }

  global.AxonRedact = { redactText, redactUrl, isBlockedCaptureUrl, blockedCaptureReason };
})(typeof self !== "undefined" ? self : globalThis);
