use crate::facts::inline_text;
use crate::manifest::{Dep, dependency_blocks, line_for_offset, tag_value};
use crate::parser::ParseInput;

pub(super) fn deps(input: &ParseInput) -> Vec<Dep> {
    let text = inline_text(input);
    dependency_blocks(inline_text(input))
        .into_iter()
        .filter_map(|block| {
            let group = tag_value(block, "groupId")?;
            let artifact = tag_value(block, "artifactId")?;
            let (line, quote) = source_evidence(text, artifact);
            Some(Dep {
                parser_id: "maven_pom",
                ecosystem: "maven",
                scope: "dependencies",
                fact_kind: "dependency",
                candidate_kind: "manifest_dependency",
                name: format!("{group}:{artifact}"),
                version: tag_value(block, "version").map(ToOwned::to_owned),
                line,
                quote,
            })
        })
        .collect()
}

/// Java toolchain version pinned via `<properties>` — checks
/// `maven.compiler.release`, `maven.compiler.source`, then `java.version`
/// (first match wins), satisfying the parsing contract's Maven/JVM family
/// toolchain requirement.
pub(super) fn toolchain(input: &ParseInput) -> Vec<Dep> {
    let text = inline_text(input);
    for tag in [
        "maven.compiler.release",
        "maven.compiler.source",
        "java.version",
    ] {
        let Some(version) = tag_value(text, tag) else {
            continue;
        };
        let (line, quote) = source_evidence(text, version);
        return vec![Dep {
            parser_id: "maven_pom",
            ecosystem: "maven",
            scope: "properties",
            fact_kind: "toolchain_version",
            candidate_kind: "toolchain_version",
            name: "java".to_string(),
            version: Some(version.to_string()),
            line,
            quote,
        }];
    }
    Vec::new()
}

// Values returned by tag_value borrow the source. Cite their original line,
// preserving XML whitespace instead of reconstructing markup across lines.
fn source_evidence(text: &str, value: &str) -> (u32, String) {
    let offset = value.as_ptr() as usize - text.as_ptr() as usize;
    let line = line_for_offset(text, offset);
    let quote = text
        .lines()
        .nth(line as usize - 1)
        .unwrap_or_default()
        .trim();
    (line, quote.to_owned())
}
