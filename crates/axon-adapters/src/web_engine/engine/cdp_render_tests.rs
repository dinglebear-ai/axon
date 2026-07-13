use super::markdown_if_not_thin;
use spider_transformations::transformation::content::SelectorConfiguration;

#[test]
fn markdown_if_not_thin_honors_selector_config() {
    let html = r#"
        <main>
            <h1>Keep this page</h1>
            <p>Enough selected content to pass the thin page threshold.</p>
            <aside>Drop this excluded navigation text</aside>
        </main>
        <footer>Drop this footer too</footer>
    "#;
    let selectors = SelectorConfiguration {
        root_selector: Some("main".to_string()),
        exclude_selector: Some("aside".to_string()),
    };
    let md = markdown_if_not_thin(html, 10, Some(&selectors)).expect("markdown");

    assert!(md.contains("Keep this page"));
    assert!(!md.contains("Drop this excluded navigation text"));
    assert!(!md.contains("Drop this footer too"));
}
