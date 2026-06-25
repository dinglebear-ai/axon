use super::*;

#[test]
fn parses_multi_prefix_document() {
    let json = r#"{
        "/": [
            { "action": "wait_for", "selector": "main" },
            { "action": "scroll_y", "pixels": 2000 },
            { "action": "wait", "ms": 1500 }
        ],
        "/blog": [
            { "action": "click", "selector": "button.load-more" }
        ]
    }"#;
    let map = parse_automation_scripts(json).expect("valid document parses");
    assert_eq!(map.len(), 2);
    assert_eq!(map.get("/").map(Vec::len), Some(3));
    assert_eq!(map.get("/blog").map(Vec::len), Some(1));
}

#[test]
fn maps_every_step_to_spider_variant() {
    let json = r#"{
        "/": [
            { "action": "evaluate", "script": "1+1" },
            { "action": "click", "selector": "a" },
            { "action": "click_all", "selector": "a" },
            { "action": "wait_for", "selector": "main" },
            { "action": "wait_for_and_click", "selector": "button" },
            { "action": "wait", "ms": 10 },
            { "action": "wait_for_navigation" },
            { "action": "scroll_x", "pixels": -5 },
            { "action": "scroll_y", "pixels": 100 },
            { "action": "infinite_scroll", "times": 4 },
            { "action": "fill", "selector": "input.q", "value": "axon" },
            { "action": "screenshot", "output": "out.png", "full_page": true }
        ]
    }"#;
    let map = parse_automation_scripts(json).expect("valid document parses");
    let steps = map.get("/").expect("root prefix present");
    assert_eq!(steps.len(), 12);
    assert!(matches!(steps[0], WebAutomation::Evaluate(_)));
    assert!(matches!(steps[6], WebAutomation::WaitForNavigation));
    assert!(matches!(
        steps[11],
        WebAutomation::Screenshot {
            full_page: true,
            omit_background: false,
            ..
        }
    ));
}

#[test]
fn screenshot_defaults_are_false() {
    let json = r#"{ "/": [ { "action": "screenshot", "output": "o.png" } ] }"#;
    let map = parse_automation_scripts(json).expect("parses");
    match &map.get("/").unwrap()[0] {
        WebAutomation::Screenshot {
            full_page,
            omit_background,
            output,
        } => {
            assert!(!full_page);
            assert!(!omit_background);
            assert_eq!(output, "o.png");
        }
        other => panic!("expected screenshot, got {other:?}"),
    }
}

#[test]
fn rejects_empty_document() {
    assert!(parse_automation_scripts("{}").is_err());
}

#[test]
fn rejects_prefix_with_no_steps() {
    assert!(parse_automation_scripts(r#"{ "/": [] }"#).is_err());
}

#[test]
fn rejects_non_path_prefix() {
    // A key without a leading '/' can never match a URL path and must be rejected.
    let json = r#"{ "blog": [ { "action": "wait", "ms": 10 } ] }"#;
    assert!(parse_automation_scripts(json).is_err());
}

#[test]
fn rejects_unknown_action() {
    let json = r#"{ "/": [ { "action": "teleport", "selector": "a" } ] }"#;
    assert!(parse_automation_scripts(json).is_err());
}

#[test]
fn rejects_malformed_json() {
    assert!(parse_automation_scripts("not json").is_err());
}
