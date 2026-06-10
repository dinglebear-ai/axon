use super::*;

#[test]
fn trim_env_value_handles_escape_edge_cases() {
    // Unknown escape sequence: \n is not recognised — backslash + 'n' pass through
    // r#""value\nraw""# is the 12-char string: "value\nraw"
    assert_eq!(trim_env_value(r#""value\nraw""#), r"value\nraw");
    // Terminal lone backslash: r#""a\""# is the 4-char string "a\"
    // outer quotes are stripped, inner "a\" → unescape: 'a' then '\' then EOF → "a\"
    assert_eq!(trim_env_value(r#""a\""#), "a\\");
    // \" inside double-quoted value is expanded to a literal double-quote
    // r#""say\"hi\"""# is: "say\"hi\""
    assert_eq!(trim_env_value(r#""say\"hi\"""#), r#"say"hi""#);
    // \\ inside double-quoted value is expanded to a single backslash
    // r#""one\\two""# is: "one\\two"
    assert_eq!(trim_env_value(r#""one\\two""#), r"one\two");
}

#[test]
fn format_trim_env_value_roundtrip_with_special_characters() {
    for raw in [
        "simple",
        "with spaces",
        "with#hash",
        "with$dollar",
        r#"with"quotes"#,
        "with'single",
        r"with\backslash",
        "",
    ] {
        let formatted = format_env_value(raw);
        let recovered = trim_env_value(&formatted);
        assert_eq!(
            recovered, raw,
            "round-trip failed for {raw:?}: formatted={formatted:?} recovered={recovered:?}"
        );
    }
}
