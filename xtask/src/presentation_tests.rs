use super::*;

#[test]
fn source_parses_and_hashes_deterministically() {
    let a = TokenSource::load().expect("source.json parses");
    let b = TokenSource::load().expect("source.json parses");
    assert_eq!(a.source_hash(), b.source_hash());
    assert!(!a.colors.is_empty());
}

#[test]
fn required_color_tokens_present() {
    let src = TokenSource::load().expect("source.json parses");
    let required = [
        "background",
        "surface",
        "surface_raised",
        "border",
        "divider",
        "text_primary",
        "text_secondary",
        "text_muted",
        "text_inverse",
        "accent",
        "accent_strong",
        "service_name",
        "automation",
        "success",
        "warning",
        "error",
        "info",
        "neutral",
        "waiting",
        "degraded",
        "source",
        "job",
        "graph",
        "memory",
        "artifact",
        "provider",
        "focus_ring",
        "hover",
        "selected",
        "disabled",
    ];
    for name in required {
        assert!(
            src.colors.iter().any(|c| c.name == name),
            "missing required color token: {name}"
        );
    }
}

#[test]
fn generation_is_idempotent_in_memory() {
    let src = TokenSource::load().expect("source.json parses");
    let root = Path::new("/does/not/matter");
    let first = artifacts(root, &src);
    let second = artifacts(root, &src);
    assert_eq!(first.len(), second.len());
    for ((path_a, content_a), (path_b, content_b)) in first.iter().zip(second.iter()) {
        assert_eq!(path_a, path_b);
        assert_eq!(content_a, content_b, "drift for {}", path_a.display());
    }
}

#[test]
fn css_emit_defines_every_color_token_in_both_modes() {
    let src = TokenSource::load().expect("source.json parses");
    let css = emit_css::render(&src);
    let dark_start = css.find(":root,\n.dark {").expect("dark block present");
    let light_start = css.find(".light {").expect("light block present");
    let dark_block = &css[dark_start..light_start];
    let light_block = &css[light_start..];
    for c in &src.colors {
        let var = format!("--axon-color-{}:", c.name.replace('_', "-"));
        assert!(dark_block.contains(&var), "dark block missing {var}");
        assert!(light_block.contains(&var), "light block missing {var}");
    }
}

#[test]
fn kotlin_emit_defines_every_color_token() {
    let src = TokenSource::load().expect("source.json parses");
    let kt = emit_kotlin::render(&src);
    for c in &src.colors {
        assert!(
            kt.contains(&super_camel(&c.name)),
            "kotlin missing {}",
            c.name
        );
    }
}

#[test]
fn rust_emit_defines_every_color_token() {
    let src = TokenSource::load().expect("source.json parses");
    let rs = emit_rust::render(&src);
    for c in &src.colors {
        assert!(
            rs.contains(&format!("pub const {}: Rgb", c.name.to_uppercase())),
            "rust missing {}",
            c.name
        );
    }
}

#[test]
fn headers_carry_contract_version_and_source_hash() {
    let src = TokenSource::load().expect("source.json parses");
    let hash = src.source_hash();
    assert!(header::css_header(&src).contains(&hash));
    assert!(header::rust_header(&src).contains(&hash));
    assert!(header::kotlin_header(&src).contains(&hash));
    assert!(header::markdown_header(&src).contains(&hash));
}

fn super_camel(name: &str) -> String {
    let mut result = String::new();
    let mut upper_next = false;
    for ch in name.chars() {
        if ch == '_' {
            upper_next = true;
            continue;
        }
        if upper_next {
            result.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}
