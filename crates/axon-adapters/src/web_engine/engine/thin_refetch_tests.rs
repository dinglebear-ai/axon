use super::*;

#[tokio::test]
async fn manifest_open_failure_is_propagated() {
    let temp = tempfile::tempdir().expect("tempdir");
    let missing_output = temp.path().join("missing");
    let error = write_refetch_results(CrawlSummary::default(), Vec::new(), &missing_output)
        .await
        .expect_err("missing output directory must not be reported as success");
    assert!(error.contains("failed to open manifest"), "{error}");
}

#[tokio::test]
async fn publication_failure_restores_markdown_and_manifest_for_retry() {
    let temp = tempfile::tempdir().expect("tempdir");
    let markdown_path = temp.path().join("page.md");
    let manifest_path = temp.path().join("manifest.jsonl");
    tokio::fs::write(&markdown_path, b"previous markdown")
        .await
        .expect("seed markdown");
    tokio::fs::write(&manifest_path, b"previous manifest\n")
        .await
        .expect("seed manifest");
    let mut manifest = tokio::fs::OpenOptions::new()
        .append(true)
        .read(true)
        .open(&manifest_path)
        .await
        .expect("open manifest");

    let error = publish_refetch_markdown(
        &mut manifest,
        &markdown_path,
        b"recovered markdown",
        b"recovered manifest\n",
        || Err("injected manifest failure".to_string()),
        || Ok(()),
        || Ok(()),
    )
    .await
    .expect_err("fault must roll publication back");
    assert!(error.contains("injected manifest failure"), "{error}");
    assert_eq!(
        tokio::fs::read(&markdown_path).await.expect("markdown"),
        b"previous markdown"
    );
    assert_eq!(
        tokio::fs::read(&manifest_path).await.expect("manifest"),
        b"previous manifest\n"
    );

    publish_refetch_markdown(
        &mut manifest,
        &markdown_path,
        b"recovered markdown",
        b"recovered manifest\n",
        || Ok(()),
        || Ok(()),
        || Ok(()),
    )
    .await
    .expect("retry succeeds");
    assert_eq!(
        tokio::fs::read(&markdown_path).await.expect("markdown"),
        b"recovered markdown"
    );
    assert_eq!(
        tokio::fs::read(&manifest_path).await.expect("manifest"),
        b"previous manifest\nrecovered manifest\n"
    );
}

#[tokio::test]
async fn publication_reports_manifest_and_markdown_restoration_failures() {
    let temp = tempfile::tempdir().expect("tempdir");
    let markdown_path = temp.path().join("page.md");
    let manifest_path = temp.path().join("manifest.jsonl");
    tokio::fs::write(&markdown_path, b"previous markdown")
        .await
        .expect("seed markdown");
    tokio::fs::write(&manifest_path, b"previous manifest\n")
        .await
        .expect("seed manifest");
    let mut manifest = tokio::fs::OpenOptions::new()
        .append(true)
        .read(true)
        .open(&manifest_path)
        .await
        .expect("open manifest");

    let error = publish_refetch_markdown(
        &mut manifest,
        &markdown_path,
        b"recovered markdown",
        b"recovered manifest\n",
        || Err("injected manifest failure".to_string()),
        || Ok(()),
        || Err("injected restoration failure".to_string()),
    )
    .await
    .expect_err("both failures must be reported");

    assert!(error.contains("injected manifest failure"), "{error}");
    assert!(error.contains("markdown restoration failed"), "{error}");
    assert!(error.contains("injected restoration failure"), "{error}");
    assert!(
        markdown_path.with_extension("thin-refetch-backup").exists(),
        "backup must be retained when restoration fails"
    );
}

#[tokio::test]
async fn partial_manifest_append_reports_set_len_rollback_failure() {
    let temp = tempfile::tempdir().expect("tempdir");
    let markdown_path = temp.path().join("page.md");
    let manifest_path = temp.path().join("manifest.jsonl");
    tokio::fs::write(&manifest_path, b"previous manifest\n")
        .await
        .expect("seed manifest");
    let mut manifest = tokio::fs::OpenOptions::new()
        .append(true)
        .read(true)
        .open(&manifest_path)
        .await
        .expect("open manifest");

    let error = publish_refetch_markdown(
        &mut manifest,
        &markdown_path,
        b"recovered markdown",
        b"partial manifest line\n",
        || Err("failure after partial append".to_string()),
        || Err("injected set_len failure".to_string()),
        || Ok(()),
    )
    .await
    .expect_err("append and rollback failures must be visible");

    assert!(error.contains("failure after partial append"), "{error}");
    assert!(error.contains("manifest rollback"), "{error}");
    assert!(error.contains("injected set_len failure"), "{error}");
    assert!(
        tokio::fs::read_to_string(&manifest_path)
            .await
            .expect("manifest")
            .contains("partial manifest line"),
        "failed rollback must leave evidence of the partial append"
    );
}

#[tokio::test]
async fn temporary_cleanup_failure_is_aggregated_with_primary_error() {
    let temp = tempfile::tempdir().expect("tempdir");
    let missing = temp.path().join("already-missing.tmp");
    let error = aggregate_temp_cleanup("primary publication failure".to_string(), &missing).await;
    assert!(error.contains("primary publication failure"), "{error}");
    assert!(error.contains("temporary file cleanup failed"), "{error}");
}

#[test]
fn recovery_diagnostics_and_logs_strip_credentials() {
    let raw = "https://user:password@example.com/thin?token=secret#reset-secret";
    let sanitized = sanitized_url_for_log(raw);
    assert_eq!(sanitized, "https://example.com/thin?redacted");
    let diagnostic =
        CrawlDiagnostic::new("chrome_render", "failed", "failed").with_url(sanitized.clone());
    assert_eq!(diagnostic.url.as_deref(), Some(sanitized.as_str()));
    assert!(!diagnostic.url.expect("url").contains("secret"));
}

#[test]
fn chrome_diagnostic_serialization_strips_credentials() {
    let raw = "https://user:password@example.com/thin?token=secret#reset-secret";
    let diagnostic = CrawlDiagnostic::new("chrome_render", "failed", "failed")
        .with_url(sanitized_url_for_log(raw));
    let payload = serde_json::to_string(&diagnostic).expect("serialize diagnostic");
    assert!(payload.contains("example.com/thin?redacted"), "{payload}");
    for secret in ["user", "password", "token", "secret", "reset"] {
        assert!(!payload.contains(secret), "leaked {secret}: {payload}");
    }
}

#[test]
fn single_page_chrome_refetch_uses_remote_policy_and_ssrf_blacklists() {
    let mut cfg = Config {
        chrome_remote_local_policy: true,
        ..Config::default()
    };
    cfg.chrome_remote_url = Some("ws://127.0.0.1:9222/devtools/browser/test".to_string());

    let website = build_single_page_website(&cfg, "https://example.com/thin");
    let intercept = super::super::super::browser::chrome_intercept_config(&cfg);

    assert!(intercept.enabled);
    assert!(intercept.remote_local_policy);
    assert_has_loopback_pattern(
        intercept
            .blacklist_patterns
            .as_ref()
            .expect("intercept blacklist"),
    );
    assert_has_loopback_pattern(
        website
            .configuration
            .blacklist_url
            .as_ref()
            .expect("website blacklist"),
    );
}

fn assert_has_loopback_pattern(patterns: &[impl ToString]) {
    assert!(
        patterns
            .iter()
            .any(|pattern| pattern.to_string().contains("127\\.")),
        "expected loopback SSRF protection in patterns"
    );
}
