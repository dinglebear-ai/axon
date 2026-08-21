// Client-side pre-redaction + blocked-capture-scheme guard, shared by the
// service worker and popup. Server-side redaction remains authoritative.
(function (global) {
  "use strict";

  const DOCUMENTED_EXAMPLE_VALUES = new Set([
    "abc123", "token", "secret", "secret-token", "password", "pass",
    "hunter2", "changeme", "example", "test", "demo"
  ]);

  const KNOWN_SECRET_TOKEN_FULL_RE = /^(?:AIza[A-Za-z0-9_-]{35}|ya29\.[A-Za-z0-9_-]{20,}|sk-(?:proj-)?[A-Za-z0-9_-]{20,}|sk_(?:test|live)_[A-Za-z0-9_-]{20,}|github_pat_[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9_-]{20,}|atk_[A-Za-z0-9_-]{20,}|xox[bp]-[A-Za-z0-9_-]{20,}|glpat-[A-Za-z0-9_-]{20,}|tvly-[A-Za-z0-9_-]{20,}|rk_(?:test|live)_[A-Za-z0-9_-]{20,})$/i;
  const KNOWN_SECRET_TOKEN_TEXT_RE = /\b(?:AIza[A-Za-z0-9_-]{35}|ya29\.[A-Za-z0-9_-]{20,}|sk-(?:proj-)?[A-Za-z0-9_-]{20,}|sk_(?:test|live)_[A-Za-z0-9_-]{20,}|github_pat_[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9_-]{20,}|atk_[A-Za-z0-9_-]{20,}|xox[bp]-[A-Za-z0-9_-]{20,}|glpat-[A-Za-z0-9_-]{20,}|tvly-[A-Za-z0-9_-]{20,}|rk_(?:test|live)_[A-Za-z0-9_-]{20,})\b/gi;
  const JWT_FULL_RE = /^eyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}$/;
  const JWT_TEXT_RE = /\beyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\b/g;

  function normalizeFieldName(field) {
    return String(field || "")
      .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
      .replace(/[-.\s]+/g, "_")
      .toLowerCase();
  }

  function isBenignSecurityField(field) {
    return /_(?:count|estimate|policy|status|type|enabled|identifier)$/.test(field)
      || ["tokenizer", "tokenization", "token_budget", "page_token", "next_page_token", "continuation_token", "pagination_token", "cursor_token"].includes(field);
  }

  function isSensitiveField(field) {
    const normalized = normalizeFieldName(field);
    if (isBenignSecurityField(normalized)) return false;
    return ["token", "secret", "credential", "credentials", "password", "passwd", "api_key", "apikey", "access_token", "refresh_token", "id_token", "private_key", "client_secret", "authorization", "proxy_authorization"].includes(normalized)
      || /_(?:token|secret|password|passwd|api_key|apikey|private_key|credential|credentials)$/.test(normalized);
  }

  function cleanedValue(value) {
    return String(value || "").trim().replace(/^["'`]|["'`,;]$/g, "");
  }

  function isDocumentedExampleValue(value) {
    const cleaned = cleanedValue(value);
    if (!cleaned || cleaned === "...") return true;
    if ((cleaned.startsWith("<") && cleaned.endsWith(">")) || (cleaned.startsWith("${") && cleaned.endsWith("}")) || (cleaned.startsWith("{{") && cleaned.endsWith("}}"))) return true;
    const lower = cleaned.toLowerCase();
    return DOCUMENTED_EXAMPLE_VALUES.has(lower) || lower.startsWith("your-") || lower.startsWith("your_") || lower.startsWith("replace-") || lower.startsWith("replace_");
  }

  function looksOpaqueCredential(value) {
    const cleaned = cleanedValue(value);
    return cleaned.length >= 20
      && /^[A-Za-z0-9_.-]+$/.test(cleaned)
      && /[A-Za-z]/.test(cleaned)
      && /[0-9]/.test(cleaned)
      && new Set(cleaned).size >= 8;
  }

  function looksConcreteSecret(field, value) {
    const cleaned = cleanedValue(value);
    if (isDocumentedExampleValue(cleaned)) return false;
    if (KNOWN_SECRET_TOKEN_FULL_RE.test(cleaned) || JWT_FULL_RE.test(cleaned)) return true;
    const normalized = normalizeFieldName(field);
    if (normalized.includes("password") || normalized.endsWith("passwd")) return cleaned.length >= 12;
    return isSensitiveField(normalized) && looksOpaqueCredential(cleaned);
  }

  function looksConcreteBearer(value) {
    const cleaned = cleanedValue(value);
    return !isDocumentedExampleValue(cleaned)
      && (KNOWN_SECRET_TOKEN_FULL_RE.test(cleaned) || JWT_FULL_RE.test(cleaned) || (cleaned.length >= 24 && looksOpaqueCredential(cleaned)));
  }

  function cookieValueIsSecret(value) {
    return String(value || "").split(";").map((part) => part.trim()).some((part) => {
      const idx = part.indexOf("=");
      if (idx <= 0) return false;
      const key = normalizeFieldName(part.slice(0, idx));
      const raw = part.slice(idx + 1).trim();
      if (!raw || isDocumentedExampleValue(raw)) return false;
      return key.includes("session") || key.includes("csrf") || key.includes("xsrf") || isSensitiveField(key) || KNOWN_SECRET_TOKEN_FULL_RE.test(raw) || looksOpaqueCredential(raw);
    });
  }

  function documentedUrlCredentials(user, password) {
    return ["user", "username"].includes(String(user || "").toLowerCase()) && isDocumentedExampleValue(password);
  }

  const PATTERNS = [
    { name: "bearer_token", re: /\b(bearer\s+)([^\s'";,]+)/gi, replace: (m, prefix, value) => looksConcreteBearer(value) ? `${prefix}[REDACTED]` : m },
    { name: "auth_header", re: /\b(authorization\s*:\s*)(?:(bearer|basic|token)\s+)?([^\s'";,]+)/gi, replace: (m, prefix, scheme, value) => (String(scheme || "").toLowerCase() === "basic" || looksConcreteBearer(value)) && !isDocumentedExampleValue(value) ? `${prefix}${scheme ? `${scheme} ` : ""}[REDACTED]` : m },
    { name: "cookie_header", re: /\b((?:set-)?cookie\s*:\s*)([^\r\n'"]+)/gim, replace: (m, prefix, value) => cookieValueIsSecret(value) ? `${prefix}[REDACTED]` : m },
    { name: "url_credentials", re: /\b([a-z][a-z0-9+.-]*:\/\/)([^\s/@]+):([^\s/@]+)@/gi, replace: (m, scheme, user, password) => documentedUrlCredentials(user, password) ? m : `${scheme}[REDACTED]@` },
    { name: "secret_kv", re: /\b([A-Za-z][A-Za-z0-9_-]*)(\s*[:=]\s*)(["']?)([A-Za-z0-9._-]+)\3/g, replace: (m, label, separator, _quote, value) => looksConcreteSecret(label, value) ? `${label}${separator}[REDACTED]` : m },
    { name: "known_secret_token", re: KNOWN_SECRET_TOKEN_TEXT_RE, replace: () => "[REDACTED]" },
    { name: "jwt", re: JWT_TEXT_RE, replace: () => "[REDACTED_JWT]" }
  ];

  const BLOCKED_SCHEME_RE = /^(chrome|chrome-extension|edge|about|devtools|view-source|file|data|blob):/i;

  function redactText(text) {
    let output = String(text || "");
    const redactions = [];
    for (const pattern of PATTERNS) {
      const before = output;
      pattern.re.lastIndex = 0;
      output = output.replace(pattern.re, pattern.replace);
      if (output !== before) redactions.push(pattern.name);
    }
    return { text: output, redactions };
  }

  function isSensitiveQueryKey(key) {
    const normalized = normalizeFieldName(key);
    if (isBenignSecurityField(normalized) || ["x_amz_algorithm", "x_amz_date", "x_amz_expires", "x_amz_signedheaders", "x_goog_algorithm", "x_goog_date", "x_goog_expires", "x_goog_signedheaders"].includes(normalized)) return false;
    return isSensitiveField(key) || ["auth", "authorization", "code", "jwt", "key", "access_key", "awsaccesskeyid", "sig", "signature", "x_amz_signature", "x_amz_credential", "x_amz_security_token", "x_goog_signature", "x_goog_credential"].includes(normalized) || normalized.endsWith("_signature");
  }

  function redactUrl(url) {
    const value = String(url || "");
    try {
      const parsed = new URL(value);
      const hadCredentials = Boolean(parsed.username || parsed.password);
      if (hadCredentials) {
        parsed.username = "";
        parsed.password = "";
      }
      const safeParams = new URLSearchParams();
      for (const [key, rawValue] of parsed.searchParams.entries()) {
        const redactedValue = redactText(rawValue).text;
        safeParams.append(key, isSensitiveQueryKey(key) || redactedValue !== rawValue ? "REDACTED" : rawValue);
      }
      parsed.search = safeParams.toString();
      const rendered = parsed.toString();
      if (!hadCredentials) return rendered;
      const scheme = `${parsed.protocol}//`;
      const rest = rendered.startsWith(scheme) ? rendered.slice(scheme.length) : rendered;
      return `${scheme}[REDACTED]@${rest}`;
    } catch {
      return redactText(value).text;
    }
  }

  function isBlockedCaptureUrl(url) {
    return BLOCKED_SCHEME_RE.test(String(url || "").trim());
  }

  function blockedCaptureReason(url) {
    if (!isBlockedCaptureUrl(url)) return null;
    const scheme = (String(url || "").match(BLOCKED_SCHEME_RE) || [])[1] || "this";
    return `Axon can't capture ${scheme}: pages — only http:// and https:// tabs can be scraped, crawled, or remembered.`;
  }

  global.AxonRedact = { redactText, redactUrl, isBlockedCaptureUrl, blockedCaptureReason };
})(typeof self !== "undefined" ? self : globalThis);
