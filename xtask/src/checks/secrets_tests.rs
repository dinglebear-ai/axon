use super::*;

// ---------------------------------------------------------------------------
// is_placeholder
// ---------------------------------------------------------------------------

#[test]
fn placeholder_angle_brackets() {
    assert!(is_placeholder("<your-token>"));
}
#[test]
fn placeholder_your_prefix() {
    assert!(is_placeholder("your-api-key"));
}
#[test]
fn placeholder_xxx() {
    assert!(is_placeholder("xxxxxxxxxx"));
}
#[test]
fn placeholder_changeme() {
    assert!(is_placeholder("changeme"));
}
#[test]
fn placeholder_redacted() {
    assert!(is_placeholder("redacted"));
}
#[test]
fn placeholder_shell_ref() {
    assert!(is_placeholder("$MY_TOKEN"));
}
#[test]
fn placeholder_template() {
    assert!(is_placeholder("{{my_secret}}"));
}
#[test]
fn placeholder_booleans() {
    assert!(is_placeholder("true"));
    assert!(is_placeholder("null"));
}
#[test]
fn placeholder_too_short() {
    assert!(is_placeholder("ab"));
}
#[test]
fn placeholder_ellipsis() {
    assert!(is_placeholder("abc...xyz"));
}

#[test]
fn real_values_not_placeholder() {
    // These should all pass through as potentially real
    assert!(!is_placeholder("s3cr3tP@ssw0rd"));
    assert!(!is_placeholder("MyR3alT0ken123"));
    assert!(!is_placeholder("Buzzaroo")); // the real-world case we missed
    assert!(!is_placeholder("p4ssw0rd!Secure"));
}

// ---------------------------------------------------------------------------
// should_skip_path
// ---------------------------------------------------------------------------

#[test]
fn skip_binary_extensions() {
    assert!(should_skip_path("image.png"));
    assert!(should_skip_path("font.woff2"));
}
#[test]
fn skip_example_env_files() {
    assert!(should_skip_path(".env.example"));
    assert!(should_skip_path("env.sample"));
}
#[test]
fn dont_skip_text_files() {
    assert!(!should_skip_path("config.toml"));
    assert!(!should_skip_path("session.md")); // session logs MUST be scanned
    assert!(!should_skip_path("notes.txt"));
}

// ---------------------------------------------------------------------------
// shannon_entropy
// ---------------------------------------------------------------------------

#[test]
fn entropy_repeated_chars_low() {
    assert!(shannon_entropy("aaaaaa") < 0.5);
}
#[test]
fn entropy_random_string_high() {
    assert!(shannon_entropy("aB3#dE9!kL2@mN7$") > 3.0);
}

// ---------------------------------------------------------------------------
// scan_file helpers
// ---------------------------------------------------------------------------

fn run_scan(content: &str) -> Vec<String> {
    // gitleaks:allow
    let definite = build_definite_regexes();
    let contextual = build_contextual_regex();
    let mut hits = Vec::new();
    scan_file("test.md", content, &definite, &contextual, &mut hits);
    hits
}

// ── Definite pattern tests ────────────────────────────────────────────────

#[test]
fn detects_github_classic_pat() {
    // 36 alphanumeric chars after ghp_ // gitleaks:allow
    let hits = run_scan("token: ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ1234567890\n"); // gitleaks:allow
    assert!(!hits.is_empty(), "should detect GitHub classic PAT");
}

#[test]
fn detects_aws_access_key() {
    let hits = run_scan("key_id = AKIAIOSFODNN7EXAMPL3\n"); // gitleaks:allow
    assert!(!hits.is_empty(), "should detect AWS access key");
}

#[test]
fn detects_anthropic_key() {
    let key = format!("sk-ant-api03-{}", "A".repeat(95)); // gitleaks:allow
    let hits = run_scan(&format!("ANTHROPIC_API_KEY={key}\n")); // gitleaks:allow
    assert!(!hits.is_empty(), "should detect Anthropic API key");
}

#[test]
fn detects_openai_key() {
    let key = format!("sk-{}", "A".repeat(48)); // gitleaks:allow
    let hits = run_scan(&format!("OPENAI_API_KEY={key}\n")); // gitleaks:allow
    assert!(!hits.is_empty(), "should detect OpenAI key");
}

#[test]
fn detects_stripe_live_key() {
    let key = format!("sk_live_{}", "A".repeat(55));
    let hits = run_scan(&format!("stripe_key={key}\n")); // gitleaks:allow
    assert!(!hits.is_empty(), "should detect Stripe live key");
}

#[test]
fn detects_slack_bot_token() {
    // Token constructed at runtime so source scanners don't flag it as a real credential.
    let token = format!(
        "{}b-{}-{}-{}",
        "xox", "123456789012", "123456789012", "abcdefghijklmnopqrstuvwx"
    );
    let line = format!("SLACK_TOKEN={token}\n");
    let hits = run_scan(&line);
    assert!(!hits.is_empty(), "should detect Slack bot token");
}

#[test]
fn detects_jwt() {
    let hits = run_scan(
        "auth: eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyMTIzIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c\n", // gitleaks:allow
    ); // gitleaks:allow
    assert!(!hits.is_empty(), "should detect JWT token");
}

#[test]
fn detects_private_key_pem() {
    let hits = run_scan(
        "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQ...\n-----END RSA PRIVATE KEY-----\n", // gitleaks:allow
    ); // gitleaks:allow
    assert!(!hits.is_empty(), "should detect PEM private key");
}

#[test]
fn detects_db_connection_string_with_password() {
    let hits = run_scan("DATABASE_URL=postgresql://user:s3cr3tp4ss@localhost:5432/mydb\n"); // gitleaks:allow
    assert!(
        !hits.is_empty(),
        "should detect DB connection string with credentials"
    );
}

#[test]
fn detects_http_url_with_credentials() {
    let hits = run_scan("url: https://apiuser:Xk9#mL2@api.example.com/v1\n"); // gitleaks:allow
    assert!(!hits.is_empty(), "should detect HTTP URL with credentials");
}

// ── Contextual pattern tests ──────────────────────────────────────────────

#[test]
fn detects_contextual_mcp_token() {
    // The Buzzaroo case — MCP_HTTP_TOKEN with a real-looking value
    let hits =
        run_scan("- Env included `AXON_MCP_HTTP_TOKEN=Buzzaroo`, `AXON_COLLECTION=cortex`\n"); // gitleaks:allow
    assert!(!hits.is_empty(), "should flag MCP_HTTP_TOKEN assignment");
}

#[test]
fn detects_contextual_api_key() {
    let hits = run_scan("API_KEY=AbCdEf1234567890GhIjKl\n"); // gitleaks:allow
    assert!(!hits.is_empty(), "real-looking API_KEY should be flagged");
}

#[test]
fn detects_contextual_password() {
    let hits = run_scan("password: MySecureP@ss2024!\n"); // gitleaks:allow
    assert!(!hits.is_empty(), "real-looking password should be flagged");
}

// ── Inline allowlist ──────────────────────────────────────────────────────

#[test]
fn respects_gitleaks_allow_comment() {
    let hits = run_scan("API_KEY=RealL00kingV@lue # gitleaks:allow\n");
    assert!(
        hits.is_empty(),
        "gitleaks:allow should suppress the finding"
    );
}

// ── Clean content ─────────────────────────────────────────────────────────

#[test]
fn ignores_placeholder_assignments() {
    let content = concat!(
        "API_KEY=your-api-key-here\n", // gitleaks:allow
        "SECRET=<redacted>\n",
        "PASSWORD=changeme\n",
        "TOKEN=$MY_TOKEN\n",
        "KEY={{secret_key}}\n",
    );
    let hits = run_scan(content); // gitleaks:allow
    assert!(hits.is_empty(), "placeholder values should not be flagged");
}

#[test]
fn ignores_empty_env_example_style() {
    let content = "GITHUB_TOKEN=\nTAVILY_API_KEY=\nHF_TOKEN=\n"; // gitleaks:allow
    let hits = run_scan(content); // gitleaks:allow
    assert!(hits.is_empty(), "empty values should not be flagged");
}

#[test]
fn clean_session_log_has_no_hits() {
    let content = "# Session log\n\nWe ran `axon scrape` and it worked fine.\n";
    let hits = run_scan(content); // gitleaks:allow
    assert!(hits.is_empty());
}
