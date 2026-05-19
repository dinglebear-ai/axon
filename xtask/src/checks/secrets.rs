//! Staged-file secret scanner.
//!
//! Three detection strategies:
//!
//! 1. **Definite** — prefix/format-anchored patterns that are unambiguously
//!    real credentials (GitHub PATs, Stripe live keys, private key PEM blocks,
//!    JWT tokens, etc.). Organized by provider/category.
//!
//! 2. **Contextual** — `KEY=value` or `KEY: value` assignments where the key
//!    name looks secret-like AND the value passes an entropy check, is long
//!    enough, and is not a placeholder.
//!
//! 3. **High-entropy strings** — long (≥40 char) hex/base64 blobs directly
//!    assigned to secret-looking keys (catches generic API keys that don't
//!    match any known prefix).
//!
//! Skips binary files and large files (>512 KiB). Reads *staged* content
//! (`git show :{path}`) so the check is always against what would be committed.

use anyhow::{Context, Result, bail};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Pattern tables — organized by provider / category
// ---------------------------------------------------------------------------

/// Patterns that unambiguously identify a credential when they appear anywhere.
/// Each entry is (regex, human-readable label).
static DEFINITE: &[(&str, &str)] = &[
    // ── Source control ───────────────────────────────────────────────────────
    (r"ghp_[A-Za-z0-9]{36}", "GitHub classic PAT (ghp_)"),
    (r"gho_[A-Za-z0-9]{36}", "GitHub OAuth token (gho_)"),
    (
        r"ghs_[A-Za-z0-9]{36}",
        "GitHub server-to-server token (ghs_)",
    ),
    (r"ghr_[A-Za-z0-9]{36}", "GitHub refresh token (ghr_)"),
    (r"github_pat_[A-Za-z0-9_]{82}", "GitHub fine-grained PAT"),
    (r"glpat-[A-Za-z0-9_\-]{20,}", "GitLab personal access token"),
    (
        r"glsa-[A-Za-z0-9_\-]{20,}-[A-Za-z0-9_\-]{20,}",
        "GitLab service account token",
    ),
    (r"glcbt-[A-Za-z0-9_\-]{20,}", "GitLab CI build token"),
    (r"gldt-[A-Za-z0-9_\-]{20,}", "GitLab deploy token"),
    // ── Cloud providers ──────────────────────────────────────────────────────
    (r"AKIA[0-9A-Z]{16}", "AWS access key ID"),
    (r"ASIA[0-9A-Z]{16}", "AWS temporary access key ID"),
    (r"AROA[0-9A-Z]{16}", "AWS assumed-role ID"),
    (r"AGPA[0-9A-Z]{16}", "AWS group ID (service principal)"),
    // GCP service account key files contain this pattern
    (
        r#""private_key":\s*"-----BEGIN RSA PRIVATE KEY"#,
        "GCP service account private key",
    ), // gitleaks:allow
    (
        r"ya29\.[A-Za-z0-9_\-]{100,}",
        "GCP OAuth access token (ya29.)",
    ),
    // Azure
    (
        r"DefaultEndpointsProtocol=https;AccountName=[^;]+;AccountKey=[A-Za-z0-9+/]{86}==",
        "Azure storage connection string",
    ),
    // ── AI / ML providers ────────────────────────────────────────────────────
    (r"sk-[A-Za-z0-9]{48}\b", "OpenAI API key (sk-)"),
    (
        r"sk-proj-[A-Za-z0-9_\-]{50,}",
        "OpenAI project key (sk-proj-)",
    ),
    (
        r"sk-svcacct-[A-Za-z0-9_\-]{50,}",
        "OpenAI service account key",
    ),
    (r"sk-ant-api03-[A-Za-z0-9_\-]{90,}", "Anthropic API key"),
    (
        r"sk-ant-[A-Za-z0-9_\-]{90,}",
        "Anthropic API key (alternate)",
    ),
    (r"hf_[A-Za-z0-9]{34,}", "HuggingFace token (hf_)"),
    (r"tvly-[A-Za-z0-9]{32,}", "Tavily API key (tvly-)"),
    // ── Package registries ───────────────────────────────────────────────────
    (r"npm_[A-Za-z0-9]{36}", "npm access token (npm_)"),
    (r"pypi-AgEI[A-Za-z0-9_\-]{40,}", "PyPI API token"),
    // ── Communication ────────────────────────────────────────────────────────
    (
        r"xoxb-[0-9]+-[0-9]+-[A-Za-z0-9]+",
        "Slack bot token (xoxb-)",
    ),
    (
        r"xoxp-[0-9]+-[0-9]+-[0-9]+-[A-Za-z0-9]+",
        "Slack user token (xoxp-)",
    ),
    (
        r"xoxs-[0-9]+-[0-9]+-[0-9]+-[A-Za-z0-9]+",
        "Slack session token (xoxs-)",
    ),
    (
        r"xoxa-[0-9]+-[0-9]+-[0-9]+-[A-Za-z0-9]+",
        "Slack legacy token (xoxa-)",
    ),
    (r"xoxe\.[A-Za-z0-9\-]+", "Slack SCIM token"),
    // Discord bot tokens: header.payload.signature (all base64url)
    (
        r"[MN][A-Za-z0-9]{23}\.[A-Za-z0-9_\-]{6}\.[A-Za-z0-9_\-]{27}",
        "Discord bot token",
    ),
    // Telegram bots: {bot_id}:{secret}
    (r"[0-9]{8,10}:[A-Za-z0-9_\-]{35}", "Telegram bot token"),
    // SendGrid
    (
        r"SG\.[A-Za-z0-9_\-]{22}\.[A-Za-z0-9_\-]{43}",
        "SendGrid API key (SG.)",
    ),
    // Mailgun
    (r"key-[0-9a-zA-Z]{32}", "Mailgun API key (key-)"),
    // ── Payment processors ───────────────────────────────────────────────────
    (
        r"sk_live_[A-Za-z0-9]{50,}",
        "Stripe live secret key (sk_live_)",
    ),
    (
        r"rk_live_[A-Za-z0-9]{50,}",
        "Stripe restricted key live (rk_live_)",
    ),
    (
        r"sk_test_[A-Za-z0-9]{50,}",
        "Stripe test secret key (sk_test_)",
    ),
    (
        r"sq0[a-z]{3}-[0-9A-Za-z_\-]{22}",
        "Square access token (sq0)",
    ),
    (
        r"access_token\$production\$[A-Za-z0-9]+\$[A-Za-z0-9]+",
        "Braintree token",
    ),
    (r"EAA[A-Za-z0-9]+", "PayPal/Braintree OAuth token"),
    // ── Cryptographic keys (PEM blocks) ─────────────────────────────────────
    (r"-----BEGIN RSA PRIVATE KEY-----", "RSA private key"), // gitleaks:allow
    (r"-----BEGIN EC PRIVATE KEY-----", "EC private key"),   // gitleaks:allow
    (r"-----BEGIN DSA PRIVATE KEY-----", "DSA private key"), // gitleaks:allow
    (
        r"-----BEGIN OPENSSH PRIVATE KEY-----", // gitleaks:allow
        "OpenSSH private key",
    ), // gitleaks:allow
    (r"-----BEGIN PGP PRIVATE KEY BLOCK-----", "PGP private key"), // gitleaks:allow
    (r"-----BEGIN PRIVATE KEY-----", "PKCS#8 private key"),  // gitleaks:allow
    (
        r"-----BEGIN ENCRYPTED PRIVATE KEY-----", // gitleaks:allow
        "Encrypted private key",
    ), // gitleaks:allow
    // ── Tokens with common prefixes ──────────────────────────────────────────
    // JWT: three base64url segments separated by dots, first decodes to {"alg":...}
    (
        r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
        "JWT token (eyJ...)",
    ),
    // DB connection strings and HTTP URLs with embedded credentials are checked
    // via check_embedded_url_credentials() which validates the password part.
    // ── CI / hosting ─────────────────────────────────────────────────────────
    (
        r"[Cc]ircleci[._-]?[Tt]oken[._-]?[=:]\s*[A-Za-z0-9]{40}",
        "CircleCI token",
    ),
    (r"shpat_[A-Za-z0-9]{32}", "Shopify access token (shpat_)"),
    (r"shpss_[A-Za-z0-9]{32}", "Shopify shared secret (shpss_)"),
    (r"shpca_[A-Za-z0-9]{32}", "Shopify custom app token"),
];

/// Key name patterns for contextual assignment detection.
/// When `KEY=value` or `KEY: value` appears, the value is checked for
/// entropy / length / placeholder before flagging.
static CONTEXTUAL_KEY_PATTERN: &str = concat!(
    r"(?i)(?:",
    r"api[_-]?key|apikey|",
    r"api[_-]?secret|",
    r"access[_-]?key|access[_-]?token|",
    r"auth[_-]?key|auth[_-]?token|",
    r"private[_-]?key|",
    r"secret[_-]?key|secret[_-]?token|",
    r"client[_-]?secret|",
    r"app[_-]?secret|app[_-]?key|",
    r"oauth[_-]?token|oauth[_-]?secret|",
    r"signing[_-]?key|signing[_-]?secret|",
    r"webhook[_-]?secret|",
    r"encryption[_-]?key|",
    r"jwt[_-]?secret|",
    r"mcp[_-]?http[_-]?token|",
    r"bearer[_-]?token|",
    r"refresh[_-]?token|",
    r"session[_-]?secret|session[_-]?key|",
    r"database[_-]?url|db[_-]?url|",
    r"redis[_-]?url|redis[_-]?password|",
    r"smtp[_-]?pass(?:word)?|",
    r"ftp[_-]?pass(?:word)?|",
    r"password|passwd",
    r")"
);

/// File extensions to skip outright (binary / generated).
static SKIP_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "ico", "bmp", "webp", "tiff", "woff", "woff2", "ttf", "eot",
    "otf", "pdf", "zip", "tar", "gz", "bz2", "xz", "zst", "7z", "rar", "exe", "dll", "so", "dylib",
    "a", "o", "wasm", "bin", "mp3", "mp4", "avi", "mov", "mkv", "wav", "flac", "db", "sqlite",
    "sqlite3",
];

/// File name patterns to skip (test fixtures, example files).
static SKIP_NAME_PATTERNS: &[&str] = &[".env.example", "env.example", ".env.sample", "env.sample"];

const MAX_FILE_BYTES: usize = 512 * 1024;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn check(root: &Path) -> Result<()> {
    let staged = staged_files(root)?;
    if staged.is_empty() {
        return Ok(());
    }

    let definite_compiled = build_definite_regexes();
    let contextual_re = build_contextual_regex();

    let mut hits: Vec<String> = Vec::new();

    for path in &staged {
        if should_skip_path(path) {
            continue;
        }
        let content = match staged_content(root, path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if content.len() > MAX_FILE_BYTES {
            continue;
        }
        scan_file(
            path,
            &content,
            &definite_compiled,
            &contextual_re,
            &mut hits,
        );
    }

    if hits.is_empty() {
        return Ok(());
    }

    eprintln!("[secrets] BLOCKED — potential credential(s) in staged files:");
    for h in &hits {
        eprintln!("  {h}");
    }
    eprintln!();
    eprintln!("[secrets] Redact the value and re-stage, or add an inline allowlist comment:");
    eprintln!("[secrets] # gitleaks:allow  (end of line)");
    bail!("secrets check blocked {} finding(s)", hits.len());
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

fn scan_file(
    path: &str,
    content: &str,
    definite: &[(&str, Regex)],
    contextual: &Regex,
    hits: &mut Vec<String>,
) {
    for (lineno, line) in content.lines().enumerate() {
        let lineno = lineno + 1;

        // Skip lines with an inline allowlist marker (gitleaks convention)
        if line.contains("gitleaks:allow") || line.contains("nosec") {
            continue;
        }

        // 1. Definite patterns
        for (label, re) in definite {
            if re.is_match(line) {
                hits.push(format!("{path}:{lineno}: {label}"));
            }
        }

        // 2. DB/HTTP URLs with embedded credentials (validated)
        check_embedded_url_credentials(line, path, lineno, hits);

        // 3. Contextual key=value patterns
        for cap in contextual.captures_iter(line) {
            if let Some(value) = cap.get(1).map(|m| m.as_str())
                && !is_placeholder(value)
                && looks_like_secret(value)
            {
                let key = cap
                    .get(0)
                    .map(|m| {
                        m.as_str()
                            .split(['=', ':'])
                            .next()
                            .unwrap_or("?")
                            .trim()
                            .to_uppercase()
                    })
                    .unwrap_or_else(|| "?".to_string());
                hits.push(format!("{path}:{lineno}: secret assignment ({key}=...)"));
            }
        }
    }
}

/// Check DB/HTTP URLs for embedded credentials by validating the password part.
/// Only flags when the password doesn't look like a placeholder or example.
fn check_embedded_url_credentials(line: &str, path: &str, lineno: usize, hits: &mut Vec<String>) {
    static DB_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static HTTP_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

    let db_re = DB_RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:postgresql|postgres|mysql|mongodb|redis|amqp|amqps)://[^:@\s]+:([^@\s]{4,})@",
        )
        .unwrap()
    });
    let http_re = HTTP_RE.get_or_init(|| {
        Regex::new(r"(?i)https?://[A-Za-z0-9_.+\-]+:([^@\s]{6,})@[A-Za-z0-9.\-]").unwrap()
    });

    for (re, label) in [
        (db_re, "DB connection string with credentials"),
        (http_re, "HTTP URL with embedded credentials"),
    ] {
        if let Some(password) = re.captures(line).and_then(|c| c.get(1)).map(|m| m.as_str())
            && !is_placeholder(password)
        {
            hits.push(format!("{path}:{lineno}: {label}"));
        }
    }
}

/// True when the value looks like a real secret rather than a placeholder.
/// Checks entropy and length in addition to placeholder heuristics.
fn looks_like_secret(value: &str) -> bool {
    let v = value.trim();
    // Must be long enough to be a real secret
    if v.len() < 8 {
        return false;
    }
    // High-entropy check for longer strings
    if v.len() >= 16 {
        let entropy = shannon_entropy(v);
        if entropy < 3.0 {
            return false; // too low entropy — likely a word or repeated pattern
        }
    }
    true
}

/// Returns true when a value looks like a placeholder, not a real secret.
fn is_placeholder(value: &str) -> bool {
    let v = value.trim();
    if v.is_empty() || v.len() < 4 {
        return true;
    }
    let lo = v.to_lowercase();

    // Explicit placeholder markers
    if lo.starts_with('<') || lo.ends_with('>') {
        return true;
    }
    // Common placeholder prefixes
    if lo.starts_with("your") || lo.starts_with("my-") || lo.starts_with("my_") {
        return true;
    }
    if lo.starts_with("xxx") || lo.starts_with("yyy") || lo.starts_with("zzz") {
        return true;
    }
    // Common placeholder words
    // Rust/code-specific patterns that aren't real string values
    if v.contains('(') || v.contains(')') {
        return true; // function calls like Some(value), None, Box::new(...)
    }
    for word in &[
        "replace",
        "placeholder",
        "example",
        "changeme",
        "redacted",
        "secret",
        "token",
        "password",
        "credential",
        "apikey",
        "api_key",
        "insert",
        "enter",
        "add_",
        "set_",
        "put_",
        "fill",
        "todo",
        "none",
        "null",
        "false",
        "true",
        "n/a",
        "test",
        "dummy",
        "fake",
        "mock",
        "sample",
        "demo",
        "dev",
        "staging",
        "prod",
        // common test placeholder values
        "keep",
        "also",
        "value",
        "string",
        "data",
        "content",
        "pass",
        "user",
        "host",
        "localhost",
        "local",
        "default",
        "foobar",
        "qwerty",
        "abc123",
        "admin",
    ] {
        if lo.contains(word) && v.len() < 32 {
            return true;
        }
    }
    // Ellipsis / truncation markers
    if lo.contains("...") || lo.contains("***") {
        return true;
    }
    // Shell / template variable references
    if v.starts_with('$') || v.starts_with("#{") || v.starts_with("{{") {
        return true;
    }
    // All the same character repeated (e.g. "xxxxxxxxxx")
    let chars: Vec<char> = v.chars().collect();
    if chars.len() > 2 && chars.windows(2).all(|w| w[0] == w[1]) {
        return true;
    }
    false
}

/// Shannon entropy of a string (bits per character).
fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let mut freq: HashMap<char, usize> = HashMap::new();
    for c in s.chars() {
        *freq.entry(c).or_insert(0) += 1;
    }
    let len = s.len() as f64;
    freq.values()
        .map(|&count| {
            let p = count as f64 / len;
            -p * p.log2()
        })
        .sum()
}

// ---------------------------------------------------------------------------
// Regex builders
// ---------------------------------------------------------------------------

fn build_definite_regexes() -> Vec<(&'static str, Regex)> {
    DEFINITE
        .iter()
        .map(|(pat, label)| {
            (
                *label,
                Regex::new(pat).unwrap_or_else(|e| panic!("bad pattern {pat}: {e}")),
            )
        })
        .collect()
}

fn build_contextual_regex() -> Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Concat to avoid raw-string / quote-escaping issues across editors.
        let value_capture = r"([A-Za-z0-9!@#$%^&*()\-_+/|~]{8,})";
        let pat = CONTEXTUAL_KEY_PATTERN.to_string() + r"\s*[=:]\s*" + value_capture;
        Regex::new(&pat).expect("invalid contextual pattern")
    })
    .clone()
}

// ---------------------------------------------------------------------------
// Git helpers
// ---------------------------------------------------------------------------

fn staged_files(root: &Path) -> Result<Vec<String>> {
    let out = Command::new("git")
        .args(["diff", "--cached", "--name-only", "--diff-filter=AMR"])
        .current_dir(root)
        .output()
        .context("failed to run git diff --cached")?;
    if !out.status.success() {
        bail!(
            "git diff --cached failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8(out.stdout)
        .context("git diff output is not UTF-8")?
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

fn staged_content(root: &Path, path: &str) -> Result<String> {
    let out = Command::new("git")
        .args(["show", &format!(":{path}")])
        .current_dir(root)
        .output()
        .context("failed to run git show")?;
    if !out.status.success() {
        bail!("git show :{path} failed");
    }
    // Binary files contain null bytes
    if out.stdout.contains(&0u8) {
        bail!("binary file");
    }
    String::from_utf8(out.stdout).context("file is not UTF-8")
}

fn should_skip_path(path: &str) -> bool {
    let p = Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    if SKIP_EXTENSIONS.contains(&ext.as_str()) {
        return true;
    }
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    SKIP_NAME_PATTERNS.contains(&name)
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
