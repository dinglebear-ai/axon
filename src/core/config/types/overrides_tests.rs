use super::*;

#[test]
fn apply_overrides_sets_collection() {
    let base = Config::default();
    let cfg = base.apply_overrides(&ConfigOverrides {
        collection: Some("custom-col".to_string()),
        ..ConfigOverrides::default()
    });
    assert_eq!(cfg.collection, "custom-col");
}

#[test]
fn apply_overrides_leaves_unchanged_fields() {
    let base = Config::default();
    let original_depth = base.max_depth;
    let cfg = base.apply_overrides(&ConfigOverrides {
        collection: Some("custom-col".to_string()),
        ..ConfigOverrides::default()
    });
    assert_eq!(cfg.max_depth, original_depth);
}

#[test]
fn apply_overrides_does_not_mutate_base() {
    let base = Config::default();
    let original_collection = base.collection.clone();
    let _cfg = base.apply_overrides(&ConfigOverrides {
        collection: Some("custom-col".to_string()),
        ..ConfigOverrides::default()
    });
    // base must be unchanged
    assert_eq!(base.collection, original_collection);
}

#[test]
fn apply_overrides_sets_limit() {
    let cfg = Config::default().apply_overrides(&ConfigOverrides {
        limit: Some(25),
        ..ConfigOverrides::default()
    });
    assert_eq!(cfg.search_limit, 25);
}

#[test]
fn apply_overrides_sets_render_mode() {
    let cfg = Config::default().apply_overrides(&ConfigOverrides {
        render_mode: Some(RenderMode::Chrome),
        ..ConfigOverrides::default()
    });
    assert!(matches!(cfg.render_mode, RenderMode::Chrome));
}

#[test]
fn apply_overrides_sets_scrape_fields() {
    let cfg = Config::default().apply_overrides(&ConfigOverrides {
        format: Some(ScrapeFormat::Html),
        root_selector: Some("main".to_string()),
        exclude_selector: Some("nav".to_string()),
        ..ConfigOverrides::default()
    });
    assert!(matches!(cfg.format, ScrapeFormat::Html));
    assert_eq!(cfg.root_selector.as_deref(), Some("main"));
    assert_eq!(cfg.exclude_selector.as_deref(), Some("nav"));
}

#[test]
fn apply_overrides_sets_query_filters_and_hybrid() {
    let cfg = Config::default().apply_overrides(&ConfigOverrides {
        since: Some("7d".to_string()),
        before: Some("2026-05-07".to_string()),
        hybrid_search_enabled: Some(false),
        ..ConfigOverrides::default()
    });
    assert_eq!(cfg.since.as_deref(), Some("7d"));
    assert_eq!(cfg.before.as_deref(), Some("2026-05-07"));
    assert!(!cfg.hybrid_search_enabled);
}

#[test]
fn apply_overrides_sets_ask_flags() {
    let cfg = Config::default().apply_overrides(&ConfigOverrides {
        ask_diagnostics: Some(true),
        ..ConfigOverrides::default()
    });
    assert!(cfg.ask_diagnostics);
}

#[test]
fn apply_overrides_sets_query_even_to_none() {
    let base = Config {
        query: Some("existing".to_string()),
        ..Config::default()
    };
    let cfg = base.apply_overrides(&ConfigOverrides {
        query: Some(None),
        ..ConfigOverrides::default()
    });
    assert_eq!(cfg.query, None);
}

#[test]
fn apply_overrides_sets_screenshot_fields() {
    let output = PathBuf::from("/tmp/axon-shot.png");
    let cfg = Config::default().apply_overrides(&ConfigOverrides {
        viewport_width: Some(800),
        viewport_height: Some(600),
        screenshot_full_page: Some(false),
        output_path: Some(Some(output.clone())),
        ..ConfigOverrides::default()
    });
    assert_eq!(cfg.viewport_width, 800);
    assert_eq!(cfg.viewport_height, 600);
    assert!(!cfg.screenshot_full_page);
    assert_eq!(cfg.output_path, Some(output));
}

#[test]
fn apply_overrides_sets_wait() {
    let base = Config::default();
    assert!(!base.wait);
    let cfg = base.apply_overrides(&ConfigOverrides {
        wait: Some(true),
        ..ConfigOverrides::default()
    });
    assert!(cfg.wait);
}

#[test]
fn apply_overrides_sets_embed_false() {
    let base = Config::default();
    assert!(base.embed);
    let cfg = base.apply_overrides(&ConfigOverrides {
        embed: Some(false),
        ..ConfigOverrides::default()
    });
    assert!(!cfg.embed);
}

#[test]
fn apply_overrides_sets_include_subdomains() {
    let base = Config::default();
    assert!(!base.include_subdomains);
    let cfg = base.apply_overrides(&ConfigOverrides {
        include_subdomains: Some(true),
        ..ConfigOverrides::default()
    });
    assert!(cfg.include_subdomains);
}

#[test]
fn apply_overrides_sets_respect_robots() {
    let base = Config::default();
    assert!(!base.respect_robots);
    let cfg = base.apply_overrides(&ConfigOverrides {
        respect_robots: Some(true),
        ..ConfigOverrides::default()
    });
    assert!(cfg.respect_robots);
}

#[test]
fn apply_overrides_sets_discover_sitemaps_false() {
    let base = Config::default();
    assert!(base.discover_sitemaps);
    let cfg = base.apply_overrides(&ConfigOverrides {
        discover_sitemaps: Some(false),
        ..ConfigOverrides::default()
    });
    assert!(!cfg.discover_sitemaps);
}

#[test]
fn apply_overrides_sets_sitemap_since_days() {
    let cfg = Config::default().apply_overrides(&ConfigOverrides {
        sitemap_since_days: Some(7),
        ..ConfigOverrides::default()
    });
    assert_eq!(cfg.sitemap_since_days, 7);
}

#[test]
fn apply_overrides_sets_delay_ms() {
    let cfg = Config::default().apply_overrides(&ConfigOverrides {
        delay_ms: Some(500),
        ..ConfigOverrides::default()
    });
    assert_eq!(cfg.delay_ms, 500);
}

#[test]
fn apply_overrides_sets_min_markdown_chars() {
    let cfg = Config::default().apply_overrides(&ConfigOverrides {
        min_markdown_chars: Some(500),
        ..ConfigOverrides::default()
    });
    assert_eq!(cfg.min_markdown_chars, 500);
}

#[test]
fn apply_overrides_sets_drop_thin_markdown_false() {
    let base = Config::default();
    assert!(base.drop_thin_markdown);
    let cfg = base.apply_overrides(&ConfigOverrides {
        drop_thin_markdown: Some(false),
        ..ConfigOverrides::default()
    });
    assert!(!cfg.drop_thin_markdown);
}

#[test]
fn apply_overrides_all_none_is_noop() {
    let base = Config::default();
    let cfg = base.apply_overrides(&ConfigOverrides::default());
    // Spot-check key fields are unchanged
    assert_eq!(cfg.collection, base.collection);
    assert_eq!(cfg.max_depth, base.max_depth);
    assert_eq!(cfg.search_limit, base.search_limit);
    assert_eq!(cfg.wait, base.wait);
    assert_eq!(cfg.embed, base.embed);
    assert_eq!(cfg.include_subdomains, base.include_subdomains);
    assert_eq!(cfg.respect_robots, base.respect_robots);
    assert_eq!(cfg.discover_sitemaps, base.discover_sitemaps);
    assert_eq!(cfg.delay_ms, base.delay_ms);
}
