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

use sha2::{Digest, Sha256};

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

#[tokio::test]
async fn manifest_failures_preserve_prior_valid_output() {
    for failure in [
        CommitFailurePoint::Metadata,
        CommitFailurePoint::Serialize,
        CommitFailurePoint::Write,
        CommitFailurePoint::Flush,
    ] {
        let temp = tempfile::tempdir().expect("temp output");
        let markdown_dir = temp.path().join("markdown");
        tokio::fs::create_dir_all(&markdown_dir)
            .await
            .expect("markdown directory");

        let url = "https://example.com/thin";
        let canonical = canonicalize_url_for_dedupe(url).expect("canonical URL");
        let output = markdown_dir.join(url_to_stable_filename(&canonical));
        tokio::fs::write(&output, "prior valid output")
            .await
            .expect("prior output");

        let mut summary = CrawlSummary {
            thin_pages: 1,
            ..CrawlSummary::default()
        };
        summary.thin_urls.insert(canonical.clone());
        let result = RefetchResult {
            url: url.to_string(),
            markdown: Some("replacement output".to_string()),
            diagnostic: None,
        };

        let summary =
            write_refetch_results_with_failure(summary, vec![result], temp.path(), Some(failure))
                .await;

        assert_eq!(
            tokio::fs::read_to_string(&output).await.expect("output"),
            "prior valid output",
            "{failure:?} replaced or destroyed the prior valid output"
        );
        assert_eq!(summary.thin_pages, 1);
        assert!(summary.thin_urls.contains(&canonical));
        assert_eq!(summary.markdown_files, 0);
        assert_eq!(
            tokio::fs::read_to_string(temp.path().join("manifest.jsonl"))
                .await
                .unwrap_or_default(),
            "",
            "{failure:?} left a partial manifest entry"
        );
    }
}

#[tokio::test]
async fn metadata_failure_does_not_truncate_existing_manifest() {
    let temp = tempfile::tempdir().expect("temp output");
    let markdown_dir = temp.path().join("markdown");
    tokio::fs::create_dir_all(&markdown_dir).await.unwrap();
    let manifest_path = temp.path().join("manifest.jsonl");
    let original = "prior manifest entry\n";
    tokio::fs::write(&manifest_path, original).await.unwrap();

    let summary = write_refetch_results_with_failure(
        CrawlSummary::default(),
        vec![RefetchResult {
            url: "https://example.com/thin".into(),
            markdown: Some("replacement output".into()),
            diagnostic: None,
        }],
        temp.path(),
        Some(CommitFailurePoint::Metadata),
    )
    .await;

    assert_eq!(
        tokio::fs::read_to_string(manifest_path).await.unwrap(),
        original
    );
    assert_eq!(summary.markdown_files, 0);
}

#[tokio::test]
async fn successful_refetch_commits_manifest_and_replaces_output_together() {
    let temp = tempfile::tempdir().expect("temp output");
    let markdown_dir = temp.path().join("markdown");
    tokio::fs::create_dir_all(&markdown_dir)
        .await
        .expect("markdown directory");

    let url = "https://example.com/thin";
    let canonical = canonicalize_url_for_dedupe(url).expect("canonical URL");
    let output = markdown_dir.join(url_to_stable_filename(&canonical));
    tokio::fs::write(&output, "prior valid output")
        .await
        .expect("prior output");
    let mut summary = CrawlSummary {
        thin_pages: 1,
        ..CrawlSummary::default()
    };
    summary.thin_urls.insert(canonical.clone());

    let summary = write_refetch_results(
        summary,
        vec![RefetchResult {
            url: url.to_string(),
            markdown: Some("replacement output".to_string()),
            diagnostic: None,
        }],
        temp.path(),
    )
    .await
    .expect("successful refetch");

    assert_eq!(
        tokio::fs::read_to_string(&output).await.expect("output"),
        "replacement output"
    );
    let manifest = tokio::fs::read_to_string(temp.path().join("manifest.jsonl"))
        .await
        .expect("manifest");
    let entry: ManifestEntry = serde_json::from_str(manifest.trim()).expect("manifest entry");
    assert_eq!(entry.url, canonical);
    assert_eq!(entry.markdown_chars, "replacement output".len());
    assert_eq!(summary.thin_pages, 0);
    assert!(summary.thin_urls.is_empty());
    assert_eq!(summary.markdown_files, 1);
}

#[tokio::test]
async fn restart_reconciles_interrupted_refetch_commits() {
    for (phase, expected_output, expected_manifest) in [
        (
            CommitPhase::Prepared,
            "prior valid output",
            "prior manifest\n",
        ),
        (
            CommitPhase::ManifestCommitted,
            "replacement output",
            "prior manifest\nreplacement manifest\n",
        ),
    ] {
        let temp = tempfile::tempdir().expect("temp output");
        let markdown_dir = temp.path().join("markdown");
        let transactions_dir = temp.path().join(REFETCH_TRANSACTIONS_DIR);
        tokio::fs::create_dir_all(&markdown_dir)
            .await
            .expect("markdown directory");
        tokio::fs::create_dir_all(&transactions_dir)
            .await
            .expect("transactions directory");

        let filename = "page.md";
        let output = markdown_dir.join(filename);
        let replacement = output.with_extension("refetch-tmp");
        tokio::fs::write(&output, "prior valid output")
            .await
            .expect("prior output");
        tokio::fs::write(&replacement, "replacement output")
            .await
            .expect("replacement output");
        let manifest_path = temp.path().join("manifest.jsonl");
        tokio::fs::write(&manifest_path, "prior manifest\nreplacement manifest\n")
            .await
            .expect("manifest");
        let journal = RefetchCommitJournal {
            filename: filename.to_string(),
            manifest_start: "prior manifest\n".len() as u64,
            phase,
            replacement_len: "replacement output".len() as u64,
            replacement_hash: hex::encode(Sha256::digest(b"replacement output")),
            replacement_filename: None,
            manifest_line_len: Some("replacement manifest\n".len() as u64),
            manifest_line_hash: Some(hex::encode(Sha256::digest(b"replacement manifest\n"))),
        };
        let journal_path = transactions_dir.join("interrupted.json");
        tokio::fs::write(
            &journal_path,
            serde_json::to_vec(&journal).expect("journal JSON"),
        )
        .await
        .expect("journal");

        recover_refetch_commits(temp.path()).await;

        assert_eq!(
            tokio::fs::read_to_string(&output).await.expect("output"),
            expected_output
        );
        assert_eq!(
            tokio::fs::read_to_string(&manifest_path)
                .await
                .expect("manifest"),
            expected_manifest
        );
        assert!(!replacement.exists());
        assert!(!journal_path.exists());
    }
}

#[tokio::test]
async fn committed_recovery_accepts_an_already_installed_matching_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let markdown_dir = temp.path().join("markdown");
    let transactions_dir = temp.path().join(REFETCH_TRANSACTIONS_DIR);
    tokio::fs::create_dir_all(&markdown_dir).await.unwrap();
    tokio::fs::create_dir_all(&transactions_dir).await.unwrap();
    let replacement = b"replacement output";
    tokio::fs::write(markdown_dir.join("page.md"), replacement)
        .await
        .unwrap();
    tokio::fs::write(temp.path().join("manifest.jsonl"), "prior\nreplacement\n")
        .await
        .unwrap();
    let journal_path = transactions_dir.join("interrupted.json");
    let journal = RefetchCommitJournal {
        filename: "page.md".into(),
        manifest_start: "prior\n".len() as u64,
        phase: CommitPhase::ManifestCommitted,
        replacement_len: replacement.len() as u64,
        replacement_hash: hex::encode(Sha256::digest(replacement)),
        replacement_filename: None,
        manifest_line_len: Some("replacement\n".len() as u64),
        manifest_line_hash: Some(hex::encode(Sha256::digest(b"replacement\n"))),
    };
    tokio::fs::write(&journal_path, serde_json::to_vec(&journal).unwrap())
        .await
        .unwrap();

    recover_refetch_commits(temp.path()).await;

    assert!(!journal_path.exists());
    assert_eq!(
        tokio::fs::read(markdown_dir.join("page.md")).await.unwrap(),
        replacement
    );
}

#[tokio::test]
async fn committed_recovery_rolls_back_manifest_and_retains_journal_on_content_mismatch() {
    let temp = tempfile::tempdir().unwrap();
    let markdown_dir = temp.path().join("markdown");
    let transactions_dir = temp.path().join(REFETCH_TRANSACTIONS_DIR);
    tokio::fs::create_dir_all(&markdown_dir).await.unwrap();
    tokio::fs::create_dir_all(&transactions_dir).await.unwrap();
    tokio::fs::write(markdown_dir.join("page.md"), "prior output")
        .await
        .unwrap();
    let manifest_path = temp.path().join("manifest.jsonl");
    tokio::fs::write(&manifest_path, "prior\nreplacement\n")
        .await
        .unwrap();
    let journal_path = transactions_dir.join("interrupted.json");
    let expected = b"expected replacement";
    let journal = RefetchCommitJournal {
        filename: "page.md".into(),
        manifest_start: "prior\n".len() as u64,
        phase: CommitPhase::ManifestCommitted,
        replacement_len: expected.len() as u64,
        replacement_hash: hex::encode(Sha256::digest(expected)),
        replacement_filename: None,
        manifest_line_len: Some("replacement\n".len() as u64),
        manifest_line_hash: Some(hex::encode(Sha256::digest(b"replacement\n"))),
    };
    tokio::fs::write(&journal_path, serde_json::to_vec(&journal).unwrap())
        .await
        .unwrap();

    recover_refetch_commits(temp.path()).await;

    assert_eq!(
        tokio::fs::read_to_string(manifest_path).await.unwrap(),
        "prior\n"
    );
    assert!(
        journal_path.exists(),
        "recovery evidence must remain for retry"
    );
    assert_eq!(
        tokio::fs::read_to_string(markdown_dir.join("page.md"))
            .await
            .unwrap(),
        "prior output"
    );
}

#[tokio::test]
async fn prepared_recovery_never_truncates_a_later_manifest_append() {
    let temp = tempfile::tempdir().unwrap();
    let transactions_dir = temp.path().join(REFETCH_TRANSACTIONS_DIR);
    tokio::fs::create_dir_all(temp.path().join("markdown"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(&transactions_dir).await.unwrap();
    let prior = "prior\n";
    let concurrent = "concurrent writer\n";
    tokio::fs::write(
        temp.path().join("manifest.jsonl"),
        format!("{prior}{concurrent}"),
    )
    .await
    .unwrap();
    let journal_path = transactions_dir.join("prepared.json");
    let journal = RefetchCommitJournal {
        filename: "page.md".into(),
        manifest_start: prior.len() as u64,
        phase: CommitPhase::Prepared,
        replacement_len: 3,
        replacement_hash: hex::encode(Sha256::digest(b"new")),
        replacement_filename: None,
        manifest_line_len: None,
        manifest_line_hash: None,
    };
    tokio::fs::write(&journal_path, serde_json::to_vec(&journal).unwrap())
        .await
        .unwrap();

    recover_refetch_commits(temp.path()).await;

    assert_eq!(
        tokio::fs::read_to_string(temp.path().join("manifest.jsonl"))
            .await
            .unwrap(),
        format!("{prior}{concurrent}")
    );
}

#[tokio::test]
async fn concurrent_preparations_for_one_url_use_unique_temp_files() {
    let temp = tempfile::tempdir().unwrap();
    let markdown_dir = temp.path().join("markdown");
    tokio::fs::create_dir_all(&markdown_dir).await.unwrap();
    let result = || RefetchResult {
        url: "https://example.com/same".into(),
        markdown: Some("new".into()),
        diagnostic: None,
    };

    let first = prepare_refetch(result(), &markdown_dir, temp.path(), 0, None)
        .await
        .unwrap();
    let second = prepare_refetch(result(), &markdown_dir, temp.path(), 0, None)
        .await
        .unwrap();

    assert_ne!(first.tmp_path, second.tmp_path);
    assert!(first.tmp_path.exists());
    assert!(second.tmp_path.exists());
}

#[tokio::test]
async fn recovery_rejects_non_component_replacement_filenames_before_filesystem_use() {
    for malicious in [".", "nested/replacement.tmp"] {
        assert_rejected_replacement_filename(malicious, None).await;
    }

    let parent_temp = tempfile::tempdir().unwrap();
    let output_dir = parent_temp.path().join("output");
    tokio::fs::create_dir(&output_dir).await.unwrap();
    let parent_sentinel = output_dir.join("outside-parent-sentinel");
    tokio::fs::write(&parent_sentinel, "keep me").await.unwrap();
    assert_rejected_replacement_filename(
        "../outside-parent-sentinel",
        Some((&output_dir, &parent_sentinel)),
    )
    .await;

    let absolute_temp = tempfile::tempdir().unwrap();
    let absolute_sentinel = absolute_temp.path().join("outside-absolute-sentinel");
    tokio::fs::write(&absolute_sentinel, "keep me")
        .await
        .unwrap();
    let absolute_name = absolute_sentinel.to_string_lossy().into_owned();
    assert_rejected_replacement_filename(
        &absolute_name,
        Some((absolute_temp.path(), &absolute_sentinel)),
    )
    .await;
}

async fn assert_rejected_replacement_filename(
    replacement_filename: &str,
    external: Option<(&Path, &Path)>,
) {
    let owned_temp;
    let output_dir = if let Some((output_dir, _)) = external {
        output_dir
    } else {
        owned_temp = tempfile::tempdir().unwrap();
        owned_temp.path()
    };
    let markdown_dir = output_dir.join("markdown");
    let transactions_dir = output_dir.join(REFETCH_TRANSACTIONS_DIR);
    tokio::fs::create_dir_all(&markdown_dir).await.unwrap();
    tokio::fs::create_dir_all(&transactions_dir).await.unwrap();
    tokio::fs::write(output_dir.join("manifest.jsonl"), "prior\n")
        .await
        .unwrap();
    let journal_path = transactions_dir.join("malicious.json");
    let journal = RefetchCommitJournal {
        filename: "page.md".into(),
        manifest_start: "prior\n".len() as u64,
        phase: CommitPhase::Prepared,
        replacement_len: 0,
        replacement_hash: hex::encode(Sha256::digest([])),
        replacement_filename: Some(replacement_filename.into()),
        manifest_line_len: None,
        manifest_line_hash: None,
    };
    tokio::fs::write(&journal_path, serde_json::to_vec(&journal).unwrap())
        .await
        .unwrap();

    recover_refetch_commits(output_dir).await;

    assert!(
        journal_path.exists(),
        "invalid journal must remain for diagnosis"
    );
    assert_eq!(
        tokio::fs::read_to_string(output_dir.join("manifest.jsonl"))
            .await
            .unwrap(),
        "prior\n"
    );
    if let Some((_, sentinel)) = external {
        assert_eq!(
            tokio::fs::read_to_string(sentinel).await.unwrap(),
            "keep me",
            "recovery must not touch a path outside markdown"
        );
    }
}

#[tokio::test]
async fn output_parent_sync_failure_retains_journal_for_recovery() {
    let temp = tempfile::tempdir().unwrap();
    tokio::fs::create_dir_all(temp.path().join("markdown"))
        .await
        .unwrap();

    let summary = write_refetch_results_with_failure(
        CrawlSummary::default(),
        vec![RefetchResult {
            url: "https://example.com/durable".into(),
            markdown: Some("replacement".into()),
            diagnostic: None,
        }],
        temp.path(),
        Some(CommitFailurePoint::OutputDirectorySync),
    )
    .await;

    assert_eq!(summary.markdown_files, 0);
    let transactions = temp.path().join(REFETCH_TRANSACTIONS_DIR);
    let journal_count = std::fs::read_dir(&transactions).unwrap().count();
    assert_eq!(
        journal_count, 1,
        "journal must survive uncertain durability"
    );

    recover_refetch_commits(temp.path()).await;

    assert_eq!(std::fs::read_dir(transactions).unwrap().count(), 0);
}

#[tokio::test]
async fn successful_refetch_batch_commits_in_order_and_cleans_all_journals() {
    let temp = tempfile::tempdir().unwrap();
    tokio::fs::create_dir_all(temp.path().join("markdown"))
        .await
        .unwrap();
    let results = vec![
        RefetchResult {
            url: "https://example.com/first".into(),
            markdown: Some("first body".into()),
            diagnostic: None,
        },
        RefetchResult {
            url: "https://example.com/second".into(),
            markdown: Some("second body".into()),
            diagnostic: None,
        },
    ];

    let summary = write_refetch_results(CrawlSummary::default(), results, temp.path())
        .await
        .unwrap();

    assert_eq!(summary.markdown_files, 2);
    let manifest = tokio::fs::read_to_string(temp.path().join("manifest.jsonl"))
        .await
        .unwrap();
    assert!(manifest.find("/first").unwrap() < manifest.find("/second").unwrap());
    assert_eq!(
        std::fs::read_dir(temp.path().join(REFETCH_TRANSACTIONS_DIR))
            .unwrap()
            .count(),
        0
    );
}
