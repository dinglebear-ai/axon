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
                name: name.clone(),
                version: version.as_str().map(ToOwned::to_owned),
                line: 1,
                quote: format!("{name}: {version}"),
            });
        }
    }
    Ok(deps)
}
