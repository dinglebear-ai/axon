use super::shebang_language;

#[test]
fn shebang_detection_ignores_body_utf8_crossing_probe_boundary() {
    let mut body = "#!/bin/bash\n".to_string();
    body.push_str(&"#".repeat(511 - body.len()));
    body.push('🌍');
    assert_eq!(shebang_language(&body), Some("shell"));
}

#[test]
fn env_absolute_interpreter_and_bounded_headers_match_inventory() {
    assert_eq!(
        shebang_language("#!/usr/bin/env /usr/bin/python3\nprint('x')"),
        Some("python")
    );
    assert_eq!(
        shebang_language("#!/usr/bin/env -S python3 -u\n"),
        Some("python")
    );
    assert_eq!(
        shebang_language(&format!("#!/bin/bash{}", " ".repeat(510))),
        None
    );
    assert_eq!(shebang_language("#!/opt/unknown-tool"), None);
}
