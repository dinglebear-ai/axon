#[cfg(test)]
mod tests {
    use crate::core::content::{
        canonicalize_url, extract_loc_values, extract_robots_sitemaps, is_excluded_url_path,
    };

    #[test]
    fn extract_robots_sitemaps_returns_sorted_deduped_urls() {
        let robots = r#"
            User-agent: *
            Sitemap: https://example.com/sitemap.xml
            sitemap: https://example.com/sitemap.xml
            # sitemap: https://ignored.example/sitemap.xml
            SiteMap: https://example.com/news.xml # trailing comment
            Disallow: /private
        "#;

        let result = extract_robots_sitemaps(robots);
        let expected = vec![
            "https://example.com/news.xml".to_string(),
            "https://example.com/sitemap.xml".to_string(),
        ];
        assert_eq!(result, expected);
    }

    #[test]
    fn is_excluded_url_path_matches_expected_behavior() {
        let prefixes = vec![
            " /private/ ".to_string(),
            "/admin".to_string(),
            "/".to_string(),
        ];
        let cases = vec![
            ("https://example.com/private/area", true),
            ("https://example.com/private", true),
            ("https://example.com/admin/settings", true),
            ("https://example.com/public", false),
            ("not-a-valid-url", false),
        ];
        for (url, expected) in cases {
            assert_eq!(is_excluded_url_path(url, &prefixes), expected, "url={url}");
        }
    }

    #[test]
    fn canonicalize_url_removes_fragment_and_trailing_slash() {
        let canonical =
            canonicalize_url("https://example.com/docs/path/#section").expect("url canonicalized");
        assert_eq!(canonical, "https://example.com/docs/path");

        assert!(canonicalize_url("not a valid url").is_none());
    }

    #[test]
    fn extract_loc_values_parses_case_insensitive_loc_tags() {
        let xml = r#"
            <urlset>
              <url><loc>https://example.com/a</loc></url>
              <url><LOC> https://example.com/b?x=1&amp;y=2 </LOC></url>
              <url><loc></loc></url>
            </urlset>
        "#;
        let locs = extract_loc_values(xml);
        assert_eq!(
            locs,
            vec![
                "https://example.com/a".to_string(),
                "https://example.com/b?x=1&y=2".to_string(),
            ]
        );
    }
}
