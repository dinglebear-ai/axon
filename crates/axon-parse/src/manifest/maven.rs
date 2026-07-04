use crate::facts::inline_text;
use crate::manifest::{Dep, compact_quote, dependency_blocks, line_for_offset, tag_value};
use crate::parser::ParseInput;

pub(super) fn deps(input: &ParseInput) -> Vec<Dep> {
    let text = inline_text(input);
    dependency_blocks(inline_text(input))
        .into_iter()
        .filter_map(|block| {
            let group = tag_value(block, "groupId")?;
            let artifact = tag_value(block, "artifactId")?;
            let offset = block.as_ptr() as usize - text.as_ptr() as usize;
            Some(Dep {
                parser_id: "maven_pom",
                ecosystem: "maven",
                scope: "dependencies",
                name: format!("{group}:{artifact}"),
                version: tag_value(block, "version").map(ToOwned::to_owned),
                line: line_for_offset(text, offset),
                quote: compact_quote(block),
            })
        })
        .collect()
}
