use crate::facts::inline_text;
use crate::manifest::Dep;
use crate::parser::ParseInput;

pub(super) fn deps(input: &ParseInput) -> Vec<Dep> {
    let mut deps = Vec::new();
    let mut in_require_block = false;
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == "require (" {
            in_require_block = true;
            continue;
        }
        if in_require_block && trimmed == ")" {
            in_require_block = false;
            continue;
        }
        if !in_require_block {
            // `go 1.21` pins the language toolchain version; the newer
            // `toolchain go1.21.5` directive pins an exact Go toolchain
            // build. Both satisfy the parsing contract's Go toolchain fact
            // requirement.
            if let Some(version) = trimmed.strip_prefix("go ") {
                push_toolchain(&mut deps, "go", version.trim(), trimmed, idx as u32 + 1);
                continue;
            }
            if let Some(version) = trimmed.strip_prefix("toolchain ") {
                let version = version.trim().strip_prefix("go").unwrap_or(version.trim());
                push_toolchain(&mut deps, "go", version, trimmed, idx as u32 + 1);
                continue;
            }
        }
        let dep_line = trimmed.strip_prefix("require ").unwrap_or(trimmed);
        if dep_line.is_empty()
            || dep_line.starts_with("//")
            || (!in_require_block && dep_line == trimmed)
        {
            continue;
        }
        let mut parts = dep_line.split_whitespace();
        let Some(name) = parts.next() else { continue };
        if name.is_empty() {
            continue;
        }
        deps.push(Dep {
            parser_id: "go_mod",
            ecosystem: "go",
            scope: "require",
            fact_kind: "dependency",
            candidate_kind: "manifest_dependency",
            name: name.to_string(),
            version: parts.next().map(ToOwned::to_owned),
            line: idx as u32 + 1,
            quote: trimmed.to_string(),
        });
    }
    deps
}

fn push_toolchain(deps: &mut Vec<Dep>, name: &str, version: &str, quote: &str, line: u32) {
    if version.is_empty() {
        return;
    }
    deps.push(Dep {
        parser_id: "go_mod",
        ecosystem: "go",
        scope: "toolchain",
        fact_kind: "toolchain_version",
        candidate_kind: "toolchain_version",
        name: name.to_string(),
        version: Some(version.to_string()),
        line,
        quote: quote.to_string(),
    });
}
