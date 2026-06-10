use crate::core::llm::LlmBackendConfig;
use serde_json::{Value, json};
use std::error::Error as StdError;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const GEMINI_AUTH_FILES: &[&str] = &[
    "oauth_creds.json",
    "gemini-credentials.json",
    "google_accounts.json",
];

/// Create an isolated Gemini HOME directory for headless invocation.
///
/// Copies the user's OAuth credentials from `AXON_HEADLESS_GEMINI_HOME` (or
/// `$HOME`) into a temp dir, writes a side-effect-free settings.json, and
/// installs the axon-rag-synthesize skill so Gemini CLI can invoke it natively.
pub(super) fn prepare_gemini_home(
    config: &LlmBackendConfig,
) -> Result<TempDir, Box<dyn StdError + Send + Sync>> {
    let temp = tempfile::Builder::new()
        .prefix("axon-gemini-headless-")
        .tempdir()
        .map_err(|err| format!("failed to create isolated Gemini HOME: {err}"))?;
    let gemini_dir = temp.path().join(".gemini");
    fs::create_dir_all(&gemini_dir)?;
    fs::create_dir_all(temp.path().join(".config"))?;
    fs::create_dir_all(temp.path().join(".cache"))?;

    let source_home = gemini_source_home(config)?;
    let source_gemini = source_home.join(".gemini");
    for filename in GEMINI_AUTH_FILES {
        let src = source_gemini.join(filename);
        if src.is_file() {
            fs::copy(&src, gemini_dir.join(filename)).map_err(|err| {
                format!("failed to copy Gemini auth file {}: {err}", src.display())
            })?;
        }
    }

    let source_settings = source_gemini.join("settings.json");
    write_isolated_settings(&source_settings, &gemini_dir.join("settings.json"))?;
    write_axon_rag_synthesize_skill(&gemini_dir)?;
    Ok(temp)
}

/// Write the axon-rag-synthesize skill into the isolated Gemini home so Gemini CLI
/// can discover and invoke it via the native activate_skill tool. The SKILL.md
/// content is embedded at compile time — no separate disk location needed.
fn write_axon_rag_synthesize_skill(
    gemini_dir: &Path,
) -> Result<(), Box<dyn StdError + Send + Sync>> {
    let skill_dir = gemini_dir.join("skills").join("axon-rag-synthesize");
    fs::create_dir_all(&skill_dir)
        .map_err(|err| format!("failed to create skill directory: {err}"))?;
    fs::write(
        skill_dir.join("SKILL.md"),
        crate::vector::ops::commands::ask::synthesis_prompt::SKILL_MD,
    )
    .map_err(|err| format!("failed to write axon-rag-synthesize skill: {err}"))?;
    Ok(())
}

fn gemini_source_home(
    config: &LlmBackendConfig,
) -> Result<PathBuf, Box<dyn StdError + Send + Sync>> {
    if let Some(path) = &config.gemini_home {
        return validate_source_home(path.clone());
    }
    let home = non_empty_env("HOME").map(PathBuf::from).ok_or_else(
        || -> Box<dyn StdError + Send + Sync> {
            "HOME is required to locate Gemini CLI auth files".into()
        },
    )?;
    validate_source_home(home)
}

fn validate_source_home(path: PathBuf) -> Result<PathBuf, Box<dyn StdError + Send + Sync>> {
    let metadata = fs::symlink_metadata(&path).map_err(|err| {
        format!(
            "failed to inspect Gemini source home {}: {err}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "Gemini source home must not be a symlink: {}",
            path.display()
        )
        .into());
    }
    if !metadata.is_dir() {
        return Err(format!("Gemini source home must be a directory: {}", path.display()).into());
    }
    Ok(path)
}

/// Write an isolated settings.json to `dest`.
///
/// Starts from the user's actual `source` settings so that auth configuration
/// (security.auth.selectedType and related fields) is preserved verbatim —
/// gemini 0.41+ changed how it reads auth, and generating a from-scratch file
/// causes "Please set an Auth method" errors on newer versions.
///
/// Only the fields that cause side effects in a headless subprocess are
/// overridden: mcpServers, hooks, and context.fileName are cleared so the
/// subprocess does not attempt network connections or load host extensions.
/// The `admin` key (not a recognized gemini setting) is removed if present.
fn write_isolated_settings(
    source: &Path,
    dest: &Path,
) -> Result<(), Box<dyn StdError + Send + Sync>> {
    // Read source settings, falling back to an empty object when the file is absent,
    // unreadable, or not a JSON object (e.g. corrupted file containing a string/array).
    let mut settings: Value = source
        .is_file()
        .then(|| fs::read(source).ok())
        .flatten()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| json!({}));

    // SAFETY: guaranteed to be an object by the filter + unwrap_or_else above.
    let Some(obj) = settings.as_object_mut() else {
        unreachable!("settings is always a JSON object after filter + unwrap_or_else")
    };

    // Ensure security.auth.selectedType is set — force both intermediate keys to
    // be objects even if they exist as some other JSON type (null, string, etc.).
    if !obj.get("security").is_some_and(Value::is_object) {
        obj.insert("security".into(), json!({}));
    }
    if let Some(sec) = obj.get_mut("security").and_then(Value::as_object_mut) {
        if !sec.get("auth").is_some_and(Value::is_object) {
            sec.insert("auth".into(), json!({}));
        }
        if let Some(auth) = sec.get_mut("auth").and_then(Value::as_object_mut) {
            auth.entry("selectedType")
                .or_insert_with(|| json!("oauth-personal"));
        }
    }

    // Clear fields that would cause side effects in a headless subprocess.
    obj.insert("mcpServers".into(), json!({}));
    obj.insert("hooks".into(), json!({}));
    obj.insert("context".into(), json!({ "fileName": [] }));
    obj.remove("admin"); // not a recognized gemini 0.41+ key

    fs::write(dest, serde_json::to_vec_pretty(&settings)?)?;
    Ok(())
}

fn non_empty_env(var_name: &str) -> Option<String> {
    std::env::var(var_name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
