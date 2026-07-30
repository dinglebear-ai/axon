use super::*;

/// Shape served by www.charlestoncounty.gov/sitemap.xml: HTTP 200, but the body
/// is the site's HTML 404 page. Counting this as a parsed sitemap made map
/// discovery report success with zero URLs and suppressed the anchor fallback.
const SOFT_404_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
	<meta charset="utf-8">
	<title>404 Web Page Error</title>
</head>
<body><h1>Not Found</h1></body>
</html>"#;

/// Shape served by www.lex-co.sc.gov/sitemap.xml: a valid urlset preceded by an
/// XML prolog AND an xml-stylesheet processing instruction.
const URLSET_WITH_STYLESHEET: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<?xml-stylesheet type="text/xsl" href="/sitemap.xsl"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
<url><loc>https://lex-co.sc.gov/</loc><changefreq>daily</changefreq></url>
</urlset>"#;

#[test]
fn classifies_soft_404_html_as_not_a_sitemap() {
    assert_eq!(
        classify_sitemap_document(SOFT_404_HTML),
        SitemapDocKind::NotSitemap
    );
}

#[test]
fn classifies_urlset_behind_prolog_and_stylesheet() {
    assert_eq!(
        classify_sitemap_document(URLSET_WITH_STYLESHEET),
        SitemapDocKind::UrlSet
    );
}

#[test]
fn classifies_sitemapindex() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
<sitemap><loc>https://example.com/sitemap1.xml</loc></sitemap>
</sitemapindex>"#;
    assert_eq!(classify_sitemap_document(xml), SitemapDocKind::Index);
}

#[test]
fn root_element_match_is_case_insensitive() {
    assert_eq!(
        classify_sitemap_document("<URLSET xmlns=\"x\"></URLSET>"),
        SitemapDocKind::UrlSet
    );
    assert_eq!(
        classify_sitemap_document("<SiteMapIndex></SiteMapIndex>"),
        SitemapDocKind::Index
    );
}

#[test]
fn index_wins_over_urlset_when_both_appear() {
    // A sitemapindex body that happens to mention <urlset in a comment must
    // still classify as an index, or its children are never followed.
    let xml = "<sitemapindex><!-- not a <urlset --></sitemapindex>";
    assert_eq!(classify_sitemap_document(xml), SitemapDocKind::Index);
}

#[test]
fn empty_and_tiny_bodies_are_not_sitemaps() {
    assert_eq!(classify_sitemap_document(""), SitemapDocKind::NotSitemap);
    assert_eq!(classify_sitemap_document("<"), SitemapDocKind::NotSitemap);
    assert_eq!(
        classify_sitemap_document("not xml at all"),
        SitemapDocKind::NotSitemap
    );
}

#[test]
fn root_element_beyond_scan_window_is_not_matched() {
    // Guards the constant: padding past ROOT_ELEMENT_SCAN_BYTES must not be
    // scanned, so a root element hidden that far in is treated as absent.
    let mut xml = " ".repeat(ROOT_ELEMENT_SCAN_BYTES + 10);
    xml.push_str("<urlset></urlset>");
    assert_eq!(classify_sitemap_document(&xml), SitemapDocKind::NotSitemap);
}

#[test]
fn scan_window_covers_a_realistic_prolog() {
    // The Lexington shape sits ~150 bytes in; confirm real sitemaps are well
    // inside the window rather than relying on the previous 512-byte cap.
    assert!(URLSET_WITH_STYLESHEET.len() < ROOT_ELEMENT_SCAN_BYTES);
    assert_eq!(
        classify_sitemap_document(URLSET_WITH_STYLESHEET),
        SitemapDocKind::UrlSet
    );
}

#[test]
fn default_seed_paths_include_the_three_added_fallbacks() {
    let parsed = Url::parse("https://example.com/").expect("parse");
    let seeded: Vec<String> = sitemap_seed_queue(&parsed).into_iter().collect();
    for expected in [
        "https://example.com/sitemap.xml",
        "https://example.com/sitemap1.xml",
        "https://example.com/sitemaps.xml",
        "https://example.com/sitemap/index.xml",
    ] {
        assert!(
            seeded.iter().any(|u| u == expected),
            "missing seed path {expected}; got {seeded:?}"
        );
    }
}
