use std::collections::BTreeMap;

use serde_json::Value;

use crate::facts::inline_text;
use crate::manifest::Dep;
use crate::parser::ParseInput;

pub(super) fn deps(input: &ParseInput) -> Result<Vec<Dep>, String> {
    let text = inline_text(input);
    let root = serde_json::from_str::<Value>(text).map_err(|err| err.to_string())?;
    if !root.is_object() {
        return Ok(Vec::new());
    }
    let scopes = members(text, 0)?;
    let mut deps = Vec::new();
    for scope in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "scripts",
        "engines",
    ] {
        let Some(obj) = root.get(scope).and_then(Value::as_object) else {
            continue;
        };
        let span = scopes.get(scope).ok_or("missing manifest scope location")?;
        let locations = members(text, span.value_start)?;
        for (name, value) in obj {
            if scope == "engines" && !value.is_string() {
                continue;
            }
            let span = locations
                .get(name)
                .ok_or("missing manifest member location")?;
            // A graph evidence range is one line. For a multiline value, quote
            // its actual opening line rather than inventing a flattened pair.
            let quote = text[span.start..span.end]
                .lines()
                .next()
                .unwrap_or_default();
            let (fact_kind, candidate_kind) = match scope {
                "scripts" => ("toolchain_script", "toolchain_script"),
                "engines" => ("toolchain_version", "toolchain_version"),
                _ => ("dependency", "manifest_dependency"),
            };
            let version = value
                .as_str()
                .filter(|value| scope != "scripts" || !value.is_empty());
            deps.push(Dep {
                parser_id: "package_json",
                ecosystem: "npm",
                scope,
                fact_kind,
                candidate_kind,
                name: name.clone(),
                version: version.map(ToOwned::to_owned),
                line: 1 + text[..span.start]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count() as u32,
                quote: quote.to_owned(),
            });
        }
    }
    Ok(deps)
}

struct MemberSpan {
    start: usize,
    value_start: usize,
    end: usize,
}

/// Locate immediate object members using serde's token boundaries. Repeated
/// keys use the last occurrence, matching serde_json::Value's semantic value.
fn members(text: &str, start: usize) -> Result<BTreeMap<String, MemberSpan>, String> {
    let mut cursor = start;
    whitespace(text, &mut cursor);
    if text.as_bytes().get(cursor) != Some(&b'{') {
        return Err("manifest scope is not an object".into());
    }
    cursor += 1;
    let mut result = BTreeMap::new();
    loop {
        whitespace(text, &mut cursor);
        if text.as_bytes().get(cursor) == Some(&b'}') {
            return Ok(result);
        }
        let start = cursor;
        let key = token(text, &mut cursor)?;
        let name = key.as_str().ok_or("manifest member key is not a string")?;
        whitespace(text, &mut cursor);
        if text.as_bytes().get(cursor) != Some(&b':') {
            return Err("manifest member has no colon".into());
        }
        cursor += 1;
        whitespace(text, &mut cursor);
        let value_start = cursor;
        token(text, &mut cursor)?;
        result.insert(
            name.to_owned(),
            MemberSpan {
                start,
                value_start,
                end: cursor,
            },
        );
        whitespace(text, &mut cursor);
        match text.as_bytes().get(cursor) {
            Some(b',') => cursor += 1,
            Some(b'}') => return Ok(result),
            _ => return Err("manifest member has no delimiter".into()),
        }
    }
}

fn token(text: &str, cursor: &mut usize) -> Result<Value, String> {
    let mut stream = serde_json::Deserializer::from_str(&text[*cursor..]).into_iter::<Value>();
    let value = stream
        .next()
        .ok_or("missing manifest token")?
        .map_err(|error| error.to_string())?;
    *cursor += stream.byte_offset();
    Ok(value)
}

fn whitespace(text: &str, cursor: &mut usize) {
    while text
        .as_bytes()
        .get(*cursor)
        .is_some_and(u8::is_ascii_whitespace)
    {
        *cursor += 1;
    }
}
