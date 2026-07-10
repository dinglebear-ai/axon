use serde_json::Value;

use crate::facts::inline_text;
use crate::manifest::Dep;
use crate::parser::ParseInput;

pub(super) fn deps(input: &ParseInput) -> Result<Vec<Dep>, String> {
    let root = serde_json::from_str::<Value>(inline_text(input)).map_err(|err| err.to_string())?;
    let mut deps = Vec::new();
    for scope in ["dependencies", "devDependencies", "peerDependencies"] {
        let Some(obj) = root.get(scope).and_then(Value::as_object) else {
            continue;
        };
        for (name, version) in obj {
            deps.push(Dep {
                parser_id: "package_json",
                ecosystem: "npm",
                scope,
                fact_kind: "dependency",
                candidate_kind: "manifest_dependency",
                name: name.clone(),
                version: version.as_str().map(ToOwned::to_owned),
                line: 1,
                quote: format!("{name}: {version}"),
            });
        }
    }
    if let Some(scripts) = root.get("scripts").and_then(Value::as_object) {
        for (name, command) in scripts {
            let command = command.as_str().unwrap_or_default();
            deps.push(Dep {
                parser_id: "package_json",
                ecosystem: "npm",
                scope: "scripts",
                fact_kind: "toolchain_script",
                candidate_kind: "toolchain_script",
                name: name.clone(),
                version: (!command.is_empty()).then(|| command.to_string()),
                line: 1,
                quote: format!("{name}: {command}"),
            });
        }
    }
    // `engines` pins the required runtime toolchain versions (node, npm, ...)
    // — satisfies the parsing contract's JS/TS family toolchain requirement.
    if let Some(engines) = root.get("engines").and_then(Value::as_object) {
        for (name, range) in engines {
            let Some(range) = range.as_str() else {
                continue;
            };
            deps.push(Dep {
                parser_id: "package_json",
                ecosystem: "npm",
                scope: "engines",
                fact_kind: "toolchain_version",
                candidate_kind: "toolchain_version",
                name: name.clone(),
                version: Some(range.to_string()),
                line: 1,
                quote: format!("{name}: {range}"),
            });
        }
    }
    Ok(deps)
}
