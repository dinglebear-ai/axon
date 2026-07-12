use super::*;
use crate::web_engine::engine::canonicalize_url_for_dedupe;
use crate::web_engine::manifest::ManifestEntry;
use axon_core::http::LoopbackGuard;
use std::collections::{HashMap, HashSet};

/// Run an async test body on a current-thread runtime hosted on a 16 MB-stack
/// thread. A live `crawl_raw()` recurses deep enough through spider's pipeline to
/// overflow the default ~2 MB test-thread stack under parallel test pressure
/// (intermittent SIGABRT). Used by the two tests below that drive a real crawl.
fn block_on_big_stack<F>(fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build current-thread runtime")
                .block_on(fut);
        })
        .expect("spawn big-stack test thread")
        .join()
        .expect("big-stack test thread panicked");
}

fn entry(url: &str, hash: &str) -> ManifestEntry {
    ManifestEntry {
        url: url.to_string(),
        relative_path: format!("markdown/{}.md", url_slug(url)),
        markdown_chars: 1234,
        content_hash: Some(hash.to_string()),
        changed: true,
        structured: None,
    }
}

fn url_slug(url: &str) -> String {
    url.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

// ── reconcile_targets: the zombie-safety predicate ──────────────────────────

#[test]
fn reconcile_targets_selects_only_seeded_and_absent() {
    let mut prev = HashMap::new();
    prev.insert("https://x/a".to_string(), entry("https://x/a", "h1"));
    prev.insert("https://x/b".to_string(), entry("https://x/b", "h2"));
    prev.insert("https://x/c".to_string(), entry("https://x/c", "h3"));

    // a: seeded + absent + visited → reconcile (a genuine 304 skip)
    // b: seeded but arrived (e.g. changed, re-fetched) → NOT reconciled
    // c: arrived, never seeded → NOT reconciled
    let seeded: HashSet<String> = ["https://x/a", "https://x/b"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let arrived: HashSet<String> = ["https://x/b", "https://x/c"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    // All three were scheduled/visited this run.
    let visited: HashSet<String> = ["https://x/a", "https://x/b", "https://x/c"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let targets = reconcile_targets(&prev, &seeded, &arrived, &visited, &HashMap::new());
    assert_eq!(targets, vec!["https://x/a".to_string()]);
}

#[test]
fn reconcile_targets_empty_seed_yields_no_zombies() {
    // The safe-by-construction property: with no working seed, nothing is
    // reconciled even when previous_manifest URLs are absent from arrivals.
    let mut prev = HashMap::new();
    prev.insert("https://x/gone".to_string(), entry("https://x/gone", "h1"));
    let seeded = HashSet::new();
    let arrived = HashSet::new();
    let visited = HashSet::new();
    assert!(reconcile_targets(&prev, &seeded, &arrived, &visited, &HashMap::new()).is_empty());
}

#[test]
fn reconcile_targets_orphan_not_visited_is_excluded() {
    // A page that previously had a validator but is no longer discovered this run
    // is absent from the visited set — spider never scheduled it, so it cannot
    // have 304'd. It must NOT be reconciled (no zombie resurrection). This is the
    // PR #153 review fix: the visited-set gate distinguishes a real 304 skip from
    // a deleted/undiscovered page.
    let mut prev = HashMap::new();
    prev.insert(
        "https://x/removed".to_string(),
        entry("https://x/removed", "h1"),
    );
    let seeded: HashSet<String> = ["https://x/removed"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let arrived = HashSet::new();
    // Not in visited → not a 304 skip → excluded.
    let visited = HashSet::new();
    assert!(
        reconcile_targets(&prev, &seeded, &arrived, &visited, &HashMap::new()).is_empty(),
        "an undiscovered (not-visited) page must never be reconciled"
    );
}

// ── reconcile_unmodified: end-to-end relink + manifest append ────────────────

#[tokio::test]
async fn reconcile_unmodified_relinks_archived_markdown_and_appends_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let output_dir = tmp.path();
    let markdown_dir = output_dir.join("markdown");
    let recycling_bin = output_dir.join("markdown.old");
    tokio::fs::create_dir_all(&markdown_dir).await.unwrap();
    tokio::fs::create_dir_all(&recycling_bin).await.unwrap();

    // Previous crawl produced one page; its markdown is archived in markdown.old.
    let prev_entry = entry("https://x/stable", "hash-stable");
    let filename = Path::new(&prev_entry.relative_path).file_name().unwrap();
    tokio::fs::write(recycling_bin.join(filename), b"# stable content")
        .await
        .unwrap();

    // Fresh (empty) manifest for this run, as collect_crawl_pages would have left.
    tokio::fs::write(output_dir.join("manifest.jsonl"), b"")
        .await
        .unwrap();

    let mut previous_manifest = HashMap::new();
    previous_manifest.insert(prev_entry.url.clone(), prev_entry.clone());
    let seeded: HashSet<String> = [prev_entry.url.clone()].into_iter().collect();
    let arrived: HashSet<String> = HashSet::new(); // 304-skipped this run.
    // Visited this run (spider scheduled it and got a 304).
    let visited: HashSet<String> = [prev_entry.url.clone()].into_iter().collect();

    let reconciled = reconcile_unmodified(
        output_dir,
        &previous_manifest,
        &seeded,
        &arrived,
        &visited,
        &HashMap::new(),
    )
    .await;
    assert_eq!(reconciled, 1);

    // Markdown was relinked into the live dir.
    assert!(
        tokio::fs::try_exists(markdown_dir.join(filename))
            .await
            .unwrap()
    );

    // Manifest gained a reused (changed=false) entry for the URL.
    let manifest = tokio::fs::read_to_string(output_dir.join("manifest.jsonl"))
        .await
        .unwrap();
    let line = manifest
        .lines()
        .find(|l| l.contains("https://x/stable"))
        .unwrap();
    let parsed: ManifestEntry = serde_json::from_str(line).unwrap();
    assert_eq!(parsed.url, "https://x/stable");
    assert!(!parsed.changed, "reused entry must be marked unchanged");
    assert_eq!(parsed.content_hash.as_deref(), Some("hash-stable"));
}

#[tokio::test]
async fn reconcile_unmodified_skips_when_archive_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let output_dir = tmp.path();
    tokio::fs::create_dir_all(output_dir.join("markdown"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(output_dir.join("markdown.old"))
        .await
        .unwrap();
    tokio::fs::write(output_dir.join("manifest.jsonl"), b"")
        .await
        .unwrap();

    // Seeded + absent, but no archived markdown exists → cannot relink → skip.
    let prev_entry = entry("https://x/missing", "h");
    let mut previous_manifest = HashMap::new();
    previous_manifest.insert(prev_entry.url.clone(), prev_entry.clone());
    let seeded: HashSet<String> = [prev_entry.url.clone()].into_iter().collect();
    let arrived = HashSet::new();
    let visited: HashSet<String> = [prev_entry.url.clone()].into_iter().collect();

    let reconciled = reconcile_unmodified(
        output_dir,
        &previous_manifest,
        &seeded,
        &arrived,
        &visited,
        &HashMap::new(),
    )
    .await;
    assert_eq!(reconciled, 0);
    let manifest = tokio::fs::read_to_string(output_dir.join("manifest.jsonl"))
        .await
        .unwrap();
    assert!(manifest.trim().is_empty());
}

#[tokio::test]
async fn reconcile_unmodified_excludes_undiscovered_pages() {
    // End-to-end guard for the PR #153 review fix: a seeded page that is no longer
    // discovered (absent from the visited set) must NOT be resurrected, even
    // though its archived markdown still exists — while a genuine 304 skip
    // (seeded + visited + not arrived) IS reused.
    let tmp = tempfile::tempdir().unwrap();
    let output_dir = tmp.path();
    let markdown_dir = output_dir.join("markdown");
    let recycling_bin = output_dir.join("markdown.old");
    tokio::fs::create_dir_all(&markdown_dir).await.unwrap();
    tokio::fs::create_dir_all(&recycling_bin).await.unwrap();
    tokio::fs::write(output_dir.join("manifest.jsonl"), b"")
        .await
        .unwrap();

    let gone = entry("https://x/deleted", "h-gone");
    let gone_file = Path::new(&gone.relative_path).file_name().unwrap();
    tokio::fs::write(recycling_bin.join(gone_file), b"# old deleted content")
        .await
        .unwrap();

    let kept = entry("https://x/kept", "h-kept");
    let kept_file = Path::new(&kept.relative_path).file_name().unwrap();
    tokio::fs::write(recycling_bin.join(kept_file), b"# kept content")
        .await
        .unwrap();

    let mut previous_manifest = HashMap::new();
    previous_manifest.insert(gone.url.clone(), gone.clone());
    previous_manifest.insert(kept.url.clone(), kept.clone());

    let seeded: HashSet<String> = [gone.url.clone(), kept.url.clone()].into_iter().collect();
    let arrived: HashSet<String> = HashSet::new();
    // Only the kept URL was visited this run (a real 304); the deleted URL was
    // never scheduled.
    let visited: HashSet<String> = [kept.url.clone()].into_iter().collect();

    let reconciled = reconcile_unmodified(
        output_dir,
        &previous_manifest,
        &seeded,
        &arrived,
        &visited,
        &HashMap::new(),
    )
    .await;
    assert_eq!(reconciled, 1, "only the visited 304 skip is reconciled");

    let manifest = tokio::fs::read_to_string(output_dir.join("manifest.jsonl"))
        .await
        .unwrap();
    assert!(
        manifest.contains("https://x/kept"),
        "visited 304 page must be reused"
    );
    assert!(
        !manifest.contains("https://x/deleted"),
        "undiscovered page must not be resurrected"
    );
    assert!(
        !tokio::fs::try_exists(markdown_dir.join(gone_file))
            .await
            .unwrap(),
        "deleted page's markdown must not be relinked"
    );
}

// ── sidecar round-trip + carry-forward ──────────────────────────────────────

#[tokio::test]
async fn sidecar_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let mut data = HashMap::new();
    data.insert(
        "https://x/a".to_string(),
        EtagEntry {
            etag: Some("\"abc\"".to_string()),
            last_modified: Some("Wed, 21 Oct 2026 07:28:00 GMT".to_string()),
            ..Default::default()
        },
    );
    write_sidecar(tmp.path(), &data).await.unwrap();
    let loaded = load_sidecar(tmp.path()).await;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded["https://x/a"].etag.as_deref(), Some("\"abc\""));
}

#[tokio::test]
async fn load_sidecar_absent_is_empty_not_error() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(load_sidecar(tmp.path()).await.is_empty());
}

// ── the spider seam: seeding materializes the cache and round-trips ──────────

#[test]
fn seed_website_etag_cache_materializes_and_round_trips() {
    // This is the make-or-break test for cross-run benefit: spider's ETag cache
    // is lazily built inside crawl setup, so seeding must force materialization
    // and successfully store()/get() validators. If this fails, conditional
    // re-crawl is inert and the feature must fall back to documented-gap.
    let mut website = Website::new("http://127.0.0.1/");
    website.configuration.with_etag_cache(true);

    let mut sidecar = HashMap::new();
    sidecar.insert(
        "http://127.0.0.1/page".to_string(),
        EtagEntry {
            etag: Some("\"v1\"".to_string()),
            ..Default::default()
        },
    );

    let seeded = seed_website_etag_cache(&mut website, &sidecar);
    assert_eq!(
        seeded.len(),
        1,
        "seeding must materialize the cache and store the validator"
    );

    let cache = website
        .get_etag_cache()
        .expect("cache must exist after seeding");
    let got = cache.get("http://127.0.0.1/page");
    assert!(got.is_some(), "seeded validator must be retrievable");
    let (etag, _last_mod) = got.unwrap();
    assert_eq!(etag.as_deref(), Some("\"v1\""));

    // Conditional headers are now non-empty → spider will send If-None-Match.
    assert!(
        !cache
            .conditional_headers("http://127.0.0.1/page")
            .is_empty(),
        "conditional headers must be produced for a seeded URL"
    );
}

#[test]
fn build_next_sidecar_carries_forward_unrefreshed_validators() {
    // A URL that 304'd this run has no live-cache entry (spider returns before
    // re-storing), so carry-forward from the previous sidecar must preserve it.
    let mut website = Website::new("http://127.0.0.1/");
    website.configuration.with_etag_cache(true);
    let _ = seed_website_etag_cache(&mut website, &HashMap::new());

    let mut previous = HashMap::new();
    previous.insert(
        "http://127.0.0.1/stable".to_string(),
        EtagEntry {
            etag: Some("\"keep\"".to_string()),
            ..Default::default()
        },
    );
    // "stable" did NOT arrive this run (304-skipped) → not in arrived set.
    let arrived: HashSet<String> = HashSet::new();
    let next = build_next_sidecar(&website, &previous, &arrived);
    assert_eq!(
        next.get("http://127.0.0.1/stable")
            .and_then(|e| e.etag.as_deref()),
        Some("\"keep\""),
        "304'd URL's validator must survive into the next sidecar"
    );
}

// ── the wire seam: seeded validators survive crawl setup and spider sends
//    If-None-Match, and a 304 yields zero broadcast pages (bead axon_rust-hiyf) ──
//
// This closes the one link the in-memory seam test cannot: that the seeded cache
// survives `crawl_raw()`'s internal setup and that spider actually puts the
// conditional header on the wire. A bare `Website` is built directly (not via
// `configure_website`) so axon's SSRF blacklist does not drop the loopback mock;
// production seeds public URLs where the blacklist is irrelevant.
//
// IMPORTANT (verified against spider 2.51 source): spider only consults the ETag
// cache for *discovered links* fetched through `crawl_concurrent_raw` — the start
// URL goes through `crawl_establish`, which does not. So the test seeds the
// *linked* page, not the start page, and asserts the conditional request fires
// for the discovered link.
#[test]
#[serial_test::serial]
fn seeded_validators_drive_conditional_request_and_304_drops_page() {
    block_on_big_stack(async {
        use httpmock::prelude::*;

        let _loopback = LoopbackGuard::allow();

        let server = MockServer::start();
        let stable_path = "/stable";
        let stable_url = format!("{}{}", server.base_url(), stable_path);

        // Start page links to the stable page so spider discovers + dispatches it
        // through the concurrent fetch loop (the only path that consults the cache).
        server.mock(|when, then| {
            when.method(GET).path("/");
            then.status(200)
                .header("content-type", "text/html")
                .body(format!(
                    "<html><body><a href=\"{stable_url}\">stable</a></body></html>"
                ));
        });

        // The stable page mock only matches when If-None-Match is present and returns
        // a bodyless 304. If spider did not send the validator, this never matches and
        // `hits()` stays 0.
        let conditional = server.mock(|when, then| {
            when.method(GET)
                .path(stable_path)
                .header_exists("if-none-match");
            then.status(304);
        });

        let mut website = Website::new(&server.base_url());
        website.with_depth(2);
        website.configuration.with_etag_cache(true);

        let mut sidecar = HashMap::new();
        sidecar.insert(
            stable_url.clone(),
            EtagEntry {
                etag: Some("\"v1\"".to_string()),
                last_modified: None,
                ..Default::default()
            },
        );
        let seeded = seed_website_etag_cache(&mut website, &sidecar);
        assert_eq!(seeded.len(), 1, "validator must seed before crawl");

        let mut rx = website.subscribe(16);
        website.crawl_raw().await;
        website.unsubscribe();

        // The conditional request was actually sent (seed survived setup + header on
        // the wire) and matched the 304 mock.
        assert!(
            conditional.calls() >= 1,
            "spider must send If-None-Match for the seeded discovered link (hits={})",
            conditional.calls()
        );

        // The 304'd stable page yields no content page in the broadcast — spider drops
        // it. This is exactly the silent skip the reconciliation path recovers.
        let mut pages = Vec::new();
        while let Ok(p) = rx.try_recv() {
            pages.push(p);
        }
        let delivered_stable = pages
            .iter()
            .any(|p| p.get_url().contains(stable_path) && !p.get_html_bytes_u8().is_empty());
        assert!(
            !delivered_stable,
            "304 page must NOT be delivered with content to the collector"
        );
    });
}

// ── cross-run populate: run-1 must actually persist a non-empty sidecar
//    keyed so run-2's seed lookup hits (bead axon_rust-hiyf). ─────────────────
//
// This closes the run-1 link the hand-seeded wire test cannot: spider stores
// validators under ITS target_url string, but build_next_sidecar reads them back
// under axon's canonicalized `urls` keys. If those disagree (e.g. trailing slash)
// the sidecar persists empty and cross-run benefit is inert. We drive a real
// crawl where a discovered link returns 200 + ETag and assert the persisted
// sidecar is non-empty AND its key round-trips through a fresh seed.
#[test]
#[serial_test::serial]
fn run1_populates_sidecar_with_seedable_key() {
    block_on_big_stack(async {
        use httpmock::prelude::*;

        let _loopback = LoopbackGuard::allow();

        let server = MockServer::start();
        let leaf_path = "/leaf";
        let leaf_url = format!("{}{}", server.base_url(), leaf_path);

        server.mock(|when, then| {
            when.method(GET).path("/");
            then.status(200)
                .header("content-type", "text/html")
                .body(format!(
                    "<html><body><a href=\"{leaf_url}\">leaf</a></body></html>"
                ));
        });
        // Discovered link returns a real ETag — spider should store it.
        server.mock(|when, then| {
            when.method(GET).path(leaf_path);
            then.status(200)
                .header("content-type", "text/html")
                .header("etag", "\"leaf-v1\"")
                .body("<html><body>leaf body with enough text to not be empty</body></html>");
        });

        let mut website = Website::new(&server.base_url());
        website.with_depth(2);
        website.configuration.with_etag_cache(true);
        website.configure_setup_norobots(); // materialize cache for this bare-Website test

        let mut rx = website.subscribe(16);
        website.crawl_raw().await;
        website.unsubscribe();

        // Collect the canonicalized arrival keys exactly as the collector would.
        let mut arrived: HashSet<String> = HashSet::new();
        while let Ok(p) = rx.try_recv() {
            if let Some(c) = canonicalize_url_for_dedupe(p.get_url()) {
                arrived.insert(c);
            }
        }

        // Build the next sidecar from the live cache using canonical arrival keys —
        // the exact production path. This is the assertion that catches a key mismatch.
        let next = build_next_sidecar(&website, &HashMap::new(), &arrived);
        assert!(
            next.values()
                .any(|e| e.etag.as_deref() == Some("\"leaf-v1\"")),
            "run-1 must persist the leaf ETag under a key reachable via the canonical \
         arrival set (got keys: {:?})",
            next.keys().collect::<Vec<_>>()
        );
    });
}

// ── age-out: miss_count increments + drop + reconcile_targets guard ─────────

#[test]
fn build_next_sidecar_increments_miss_count_for_non_arrived() {
    let mut website = Website::new("http://127.0.0.1/");
    website.configuration.with_etag_cache(true);
    let _ = seed_website_etag_cache(&mut website, &HashMap::new());

    let mut previous = HashMap::new();
    previous.insert(
        "http://127.0.0.1/stable".to_string(),
        EtagEntry {
            etag: Some("\"v1\"".to_string()),
            ..Default::default()
        },
    );
    // URL did NOT arrive → miss_count should increment from 0 → 1.
    let arrived: HashSet<String> = HashSet::new();
    let next = build_next_sidecar(&website, &previous, &arrived);
    assert_eq!(
        next.get("http://127.0.0.1/stable").map(|e| e.miss_count),
        Some(1),
        "miss_count must increment for a non-arrived URL"
    );
}

#[test]
fn build_next_sidecar_resets_miss_count_on_arrival() {
    let mut website = Website::new("http://127.0.0.1/");
    website.configuration.with_etag_cache(true);
    let _ = seed_website_etag_cache(&mut website, &HashMap::new());

    let mut previous = HashMap::new();
    previous.insert(
        "http://127.0.0.1/page".to_string(),
        EtagEntry {
            etag: Some("\"v1\"".to_string()),
            miss_count: 2,
            ..Default::default()
        },
    );
    // URL arrived fresh this run → miss_count should reset to 0.
    let arrived: HashSet<String> = ["http://127.0.0.1/page".to_string()].into_iter().collect();
    let next = build_next_sidecar(&website, &previous, &arrived);
    assert_eq!(
        next.get("http://127.0.0.1/page").map(|e| e.miss_count),
        Some(0),
        "miss_count must reset to 0 when a URL arrives fresh"
    );
}

#[test]
fn build_next_sidecar_drops_aged_out_entry() {
    // AXON_ETAG_MAX_MISS_RUNS defaults to 3; set miss_count to 2 so one more
    // non-arriving run tips it over the limit.
    let mut website = Website::new("http://127.0.0.1/");
    website.configuration.with_etag_cache(true);
    let _ = seed_website_etag_cache(&mut website, &HashMap::new());

    let mut previous = HashMap::new();
    previous.insert(
        "http://127.0.0.1/stale".to_string(),
        EtagEntry {
            etag: Some("\"old\"".to_string()),
            miss_count: 2, // one more miss → 3 == max → aged out
            ..Default::default()
        },
    );
    let arrived: HashSet<String> = HashSet::new();
    let next = build_next_sidecar(&website, &previous, &arrived);
    assert!(
        !next.contains_key("http://127.0.0.1/stale"),
        "aged-out entry (miss_count >= max_miss_runs) must be dropped from the sidecar"
    );
}

#[test]
fn reconcile_targets_excludes_aged_out_urls() {
    // A URL at miss_count == max_miss_runs() must NOT be reconciled even when it
    // passes the seeded/absent/visited gates — it's about to be aged out of the sidecar.
    let url = "https://x/stale".to_string();
    let mut prev = HashMap::new();
    prev.insert(url.clone(), entry(&url, "h1"));

    let seeded: HashSet<String> = [url.clone()].into_iter().collect();
    let arrived: HashSet<String> = HashSet::new();
    let visited: HashSet<String> = [url.clone()].into_iter().collect();

    // Sidecar with miss_count at the default limit (3).
    let mut sidecar = HashMap::new();
    sidecar.insert(
        url.clone(),
        EtagEntry {
            etag: Some("\"old\"".to_string()),
            miss_count: 3,
            ..Default::default()
        },
    );

    let targets = reconcile_targets(&prev, &seeded, &arrived, &visited, &sidecar);
    assert!(
        targets.is_empty(),
        "a URL at the miss_count limit must be excluded from reconciliation"
    );
}

#[test]
fn reconcile_targets_still_reconciles_below_limit() {
    // A URL at miss_count 2 (below the default limit of 3) must still be reconciled.
    let url = "https://x/almost-stale".to_string();
    let mut prev = HashMap::new();
    prev.insert(url.clone(), entry(&url, "h1"));

    let seeded: HashSet<String> = [url.clone()].into_iter().collect();
    let arrived: HashSet<String> = HashSet::new();
    let visited: HashSet<String> = [url.clone()].into_iter().collect();

    let mut sidecar = HashMap::new();
    sidecar.insert(
        url.clone(),
        EtagEntry {
            etag: Some("\"v1\"".to_string()),
            miss_count: 2, // below the default limit of 3
            ..Default::default()
        },
    );

    let targets = reconcile_targets(&prev, &seeded, &arrived, &visited, &sidecar);
    assert_eq!(
        targets,
        vec![url],
        "URLs below the limit must still be reconciled"
    );
}
