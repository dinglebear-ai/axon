use super::*;

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
    .await;

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
