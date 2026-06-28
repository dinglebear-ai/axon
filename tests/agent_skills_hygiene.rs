use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const SKILLS_ROOT: &str = "plugins/axon/skills";
const RESOURCE_ROOTS: &[&str] = &["plugins/axon/references", "plugins/axon/examples"];
const EXPECTED_SKILLS: &[&str] = &[
    "cli",
    "company-directories",
    "competitive-intel",
    "crawl",
    "dashboard-reporting",
    "deep-research",
    "demo-walkthrough",
    "download",
    "extract",
    "knowledge-base",
    "knowledge-ingest",
    "lead-gen",
    "lead-research",
    "map",
    "market-research",
    "monitor",
    "qa",
    "research-papers",
    "scrape",
    "search",
    "seo-audit",
    "shop",
    "using-axon",
    "website-design-clone",
    "workflows",
];

#[test]
fn agent_skills_are_portable_and_well_formed() {
    let root = Path::new(SKILLS_ROOT);
    assert!(root.is_dir(), "{SKILLS_ROOT} must exist");

    let mut skill_names = BTreeSet::new();
    for entry in fs::read_dir(root).expect("read skills root") {
        let entry = entry.expect("read skill entry");
        let path = entry.path();
        let file_type = entry.file_type().expect("read skill file type");
        assert!(
            !file_type.is_symlink(),
            "{} must not be a symlink",
            path.display()
        );
        if !file_type.is_dir() {
            continue;
        }

        let skill_name = entry.file_name().to_string_lossy().into_owned();
        skill_names.insert(skill_name.clone());
        let skill_md = path.join("SKILL.md");
        assert!(skill_md.is_file(), "{} must exist", skill_md.display());
        let metadata = path.join("agents/openai.yaml");
        assert!(metadata.is_file(), "{} must exist", metadata.display());

        let text = fs::read_to_string(&skill_md).expect("read SKILL.md");
        assert!(
            !text.to_lowercase().contains("firecrawl"),
            "{} must not contain Firecrawl residue",
            skill_md.display()
        );

        let (frontmatter, body) = split_frontmatter(&text, &skill_md);
        assert_minimal_frontmatter(frontmatter, &path, &skill_md);
        assert_relative_links_resolve(body, &skill_md);
        assert_openai_metadata(&metadata, &skill_name);
    }

    let expected: BTreeSet<String> = EXPECTED_SKILLS
        .iter()
        .map(|name| name.to_string())
        .collect();
    assert_eq!(skill_names, expected, "unexpected Axon skill inventory");
}

#[test]
fn agent_skill_resources_are_portable() {
    for root in RESOURCE_ROOTS.iter().map(Path::new) {
        assert!(root.is_dir(), "{} must exist", root.display());
        assert_no_symlinks(root);

        for path in markdown_files(root) {
            let text = fs::read_to_string(&path).expect("read markdown file");
            assert!(
                !text.to_lowercase().contains("firecrawl"),
                "{} must not contain Firecrawl residue",
                path.display()
            );
            assert_relative_links_resolve(&text, &path);
        }
    }
}

fn split_frontmatter<'a>(text: &'a str, path: &Path) -> (&'a str, &'a str) {
    assert!(
        text.starts_with("---\n"),
        "{} missing frontmatter",
        path.display()
    );
    let rest = &text[4..];
    let Some(end) = rest.find("\n---\n") else {
        panic!("{} missing closing frontmatter", path.display());
    };
    (&rest[..end], &rest[end + 5..])
}

fn assert_minimal_frontmatter(frontmatter: &str, skill_dir: &Path, skill_md: &Path) {
    let mut name = None;
    let mut description = None;
    for line in frontmatter.lines() {
        if let Some(value) = line.strip_prefix("name: ") {
            name = Some(value.trim());
            continue;
        }
        if let Some(value) = line.strip_prefix("description: ") {
            description = Some(value.trim());
            continue;
        }
        panic!(
            "{} has nonportable frontmatter key or multiline metadata: {line}",
            skill_md.display()
        );
    }

    let expected_name = skill_dir
        .file_name()
        .expect("skill dir name")
        .to_string_lossy();
    assert_eq!(
        name,
        Some(expected_name.as_ref()),
        "{} name mismatch",
        skill_md.display()
    );

    let description =
        description.unwrap_or_else(|| panic!("{} missing description", skill_md.display()));
    assert!(
        !description.is_empty(),
        "{} description must not be empty",
        skill_md.display()
    );
    assert!(
        description.chars().count() <= 300,
        "{} description is too long: {} chars",
        skill_md.display(),
        description.chars().count()
    );
}

fn assert_openai_metadata(path: &Path, skill_name: &str) {
    let text = fs::read_to_string(path).expect("read agents/openai.yaml");
    assert!(
        !text.to_lowercase().contains("firecrawl"),
        "{} must not contain Firecrawl residue",
        path.display()
    );
    let metadata = parse_openai_metadata(&text, path);
    assert!(
        !metadata.display_name.is_empty(),
        "{} display_name must not be empty",
        path.display()
    );
    assert!(
        !metadata.short_description.is_empty(),
        "{} short_description must not be empty",
        path.display()
    );
    assert!(
        metadata.short_description.chars().count() <= 120,
        "{} short_description is too long",
        path.display()
    );
    assert!(
        is_hex_color(&metadata.brand_color),
        "{} brand_color must be #RRGGBB",
        path.display()
    );
    let expected_implicit = skill_name != "deep-research";
    assert_eq!(
        metadata.allow_implicit_invocation,
        expected_implicit,
        "{} policy.allow_implicit_invocation mismatch for {skill_name}",
        path.display()
    );
}

#[derive(Default)]
struct OpenAiMetadata {
    display_name: String,
    short_description: String,
    brand_color: String,
    allow_implicit_invocation: bool,
}

fn parse_openai_metadata(text: &str, path: &Path) -> OpenAiMetadata {
    let mut current_section = None;
    let mut seen_keys = BTreeSet::new();
    let mut metadata = OpenAiMetadata::default();

    for (idx, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim_end();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            current_section = Some(match line {
                "interface:" => "interface",
                "policy:" => "policy",
                _ => panic!(
                    "{}:{} unknown top-level metadata key `{line}`",
                    path.display(),
                    idx + 1
                ),
            });
            continue;
        }

        let section = current_section.unwrap_or_else(|| {
            panic!(
                "{}:{} nested metadata without section",
                path.display(),
                idx + 1
            )
        });
        let Some((key, value)) = line.trim_start().split_once(':') else {
            panic!("{}:{} malformed metadata line", path.display(), idx + 1);
        };
        let qualified_key = format!("{section}.{key}");
        assert!(
            seen_keys.insert(qualified_key.clone()),
            "{}:{} duplicate metadata key `{qualified_key}`",
            path.display(),
            idx + 1
        );
        let value = value.trim();
        match (section, key) {
            ("interface", "display_name") => metadata.display_name = unquote(value).to_string(),
            ("interface", "short_description") => {
                metadata.short_description = unquote(value).to_string()
            }
            ("interface", "brand_color") => metadata.brand_color = unquote(value).to_string(),
            ("policy", "allow_implicit_invocation") => {
                metadata.allow_implicit_invocation = match value {
                    "true" => true,
                    "false" => false,
                    _ => panic!(
                        "{}:{} allow_implicit_invocation must be boolean",
                        path.display(),
                        idx + 1
                    ),
                }
            }
            _ => panic!(
                "{}:{} unknown metadata key `{section}.{key}`",
                path.display(),
                idx + 1
            ),
        }
    }

    metadata
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

fn assert_no_symlinks(root: &Path) {
    for path in walk(root) {
        let metadata = fs::symlink_metadata(&path).expect("read metadata");
        assert!(
            !metadata.file_type().is_symlink(),
            "{} must not be a symlink",
            path.display()
        );
    }
}

fn markdown_files(root: &Path) -> Vec<PathBuf> {
    walk(root)
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect()
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(&path).expect("read dir") {
            let entry = entry.expect("read dir entry");
            let child = entry.path();
            out.push(child.clone());
            if entry.file_type().expect("read file type").is_dir() {
                stack.push(child);
            }
        }
    }
    out
}

fn assert_relative_links_resolve(content: &str, file: &Path) {
    let mut in_fence = false;
    for (idx, line) in content.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }

        let mut rest = line;
        while let Some(open) = rest.find("](") {
            rest = &rest[open + 2..];
            let Some(close) = rest.find(')') else {
                break;
            };
            let target = &rest[..close];
            rest = &rest[close + 1..];
            let target = target.split('#').next().unwrap_or("").trim();
            if target.is_empty()
                || target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
                || target.starts_with('/')
                || target.contains('[')
            {
                continue;
            }
            let resolved = file.parent().expect("file parent").join(target);
            assert!(
                resolved.exists(),
                "{}:{} has broken relative link {}",
                file.display(),
                idx + 1,
                target
            );
        }
    }
}
