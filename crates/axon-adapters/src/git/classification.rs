//! Conservative repository source classification; never executes discovered scripts.
use crate::adapter::Result;
use axon_api::source::*;
use std::fs::File;
use std::io::Read;
use std::path::Path;

const SCRIPT_PROBE_BYTES: usize = 8192;

fn named_kind(key: &str) -> Option<ContentKind> {
    let path = Path::new(key);
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "dockerfile" | "containerfile" | "makefile" | "justfile"
    ) {
        return Some(ContentKind::Code);
    }
    if [
        "readme",
        "license",
        "licence",
        "changelog",
        "contributing",
        "authors",
        "notice",
    ]
    .iter()
    .any(|prefix| name == *prefix || name.starts_with(&format!("{prefix}.")))
        && matches!(
            path.extension()
                .and_then(|x| x.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            None | Some("txt")
        )
    {
        return Some(ContentKind::PlainText);
    }
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "conf"
                    | "service"
                    | "socket"
                    | "timer"
                    | "target"
                    | "mount"
                    | "automount"
                    | "preset"
                    | "te"
                    | "fc"
            )
        })
    {
        return Some(ContentKind::PlainText);
    }
    let kind = content_kind_for(path);
    (!matches!(kind, ContentKind::PlainText)).then_some(kind)
}

pub(super) fn repository_path_candidate(key: &str) -> bool {
    named_kind(key).is_some() || script_candidate(Path::new(key))
}

fn script_candidate(path: &Path) -> bool {
    path.extension().is_none()
        && path
            .file_name()
            .and_then(|x| x.to_str())
            .is_some_and(|x| !x.starts_with('.'))
}

pub(super) fn classify_repository_file(path: &Path, key: &str) -> Result<Option<ContentKind>> {
    if let Some(kind) = named_kind(key) {
        return Ok(Some(kind));
    }
    if !script_candidate(Path::new(key)) {
        return Ok(None);
    }
    // Fixed-size probe: no unbounded reads or allocations for arbitrary repository files.
    let file = File::open(path).map_err(|err| super::fs_error("read_failed", path, err))?;
    let mut prefix = Vec::with_capacity(SCRIPT_PROBE_BYTES);
    file.take(SCRIPT_PROBE_BYTES as u64)
        .read_to_end(&mut prefix)
        .map_err(|err| super::fs_error("read_failed", path, err))?;
    // Permit a final partial UTF-8 character at the probe boundary only.
    let text = match std::str::from_utf8(&prefix) {
        Ok(text) => text,
        Err(err) if err.error_len().is_none() => {
            std::str::from_utf8(&prefix[..err.valid_up_to()]).unwrap_or("")
        }
        Err(_) => return Ok(None),
    };
    if text.contains('\0') || !recognized_shebang(text.lines().next().unwrap_or("")) {
        return Ok(None);
    }
    Ok(Some(ContentKind::Code))
}

fn recognized_shebang(line: &str) -> bool {
    axon_parse::code::shebang_language(line).is_some()
}

fn content_kind_for(path: &Path) -> ContentKind {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    match extension.to_ascii_lowercase().as_str() {
        "md" | "markdown" | "mdx" | "rst" | "adoc" | "asciidoc" => ContentKind::Markdown,
        "html" | "htm" => ContentKind::Html,
        "json" => ContentKind::Json,
        "yaml" | "yml" => ContentKind::Yaml,
        "toml" => ContentKind::Toml,
        "xml" => ContentKind::Xml,
        "rs" | "go" | "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "py" | "java" | "kt"
        | "kts" | "swift" | "c" | "cc" | "cpp" | "h" | "hpp" | "cs" | "rb" | "php" | "sh"
        | "zsh" | "fish" | "ex" | "exs" | "erl" | "hrl" | "hs" | "scala" | "sc" | "sql" | "lua"
        | "pl" | "r" | "jl" | "dart" | "vue" | "svelte" | "proto" | "css" | "scss" | "less"
        | "tf" | "nix" => ContentKind::Code,
        _ => ContentKind::PlainText,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documentation_extension_matching_remains_case_insensitive() {
        assert_eq!(named_kind("README.TXT"), Some(ContentKind::PlainText));
        assert_eq!(named_kind("LICENSE.txt"), Some(ContentKind::PlainText));
        assert_eq!(named_kind("README.PNG"), None);
    }

    #[test]
    fn recognizes_interpreters_without_executing_or_accepting_arbitrary_env_options() {
        for header in [
            "#!/usr/bin/env python3",
            "#!/usr/bin/env -S python3 -u",
            "#!/usr/bin/python3.12",
            "#!/bin/bash -e",
            "#!/usr/bin/env LC_ALL=C perl",
        ] {
            assert!(recognized_shebang(header), "{header}");
        }
        for header in [
            "#!/usr/bin/env --split-string=python3",
            "#!/opt/private-tool",
            "#!/usr/bin/env",
            "# python3",
            "#!/usr/bin/python3.notaversion",
        ] {
            assert!(!recognized_shebang(header), "{header}");
        }
        assert!(!recognized_shebang(&format!(
            "#!/bin/sh {}",
            "x".repeat(513)
        )));
    }

    #[test]
    fn bounded_probe_handles_unicode_boundary_without_admitting_binary_prefixes() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("script");
        let mut body = b"#!/bin/sh\n".to_vec();
        body.resize(SCRIPT_PROBE_BYTES - 1, b'#');
        body.extend_from_slice("🌍".as_bytes());
        std::fs::write(&script, &body).unwrap();
        assert_eq!(
            classify_repository_file(&script, "script").unwrap(),
            Some(ContentKind::Code)
        );
        body[100] = 0xff;
        std::fs::write(&script, &body).unwrap();
        assert_eq!(classify_repository_file(&script, "script").unwrap(), None);
        std::fs::write(&script, format!("#!{}\n", "x".repeat(8192))).unwrap();
        assert_eq!(classify_repository_file(&script, "script").unwrap(), None);
    }
}
