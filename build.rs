use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FALLBACK_MARKER: &str = "AXON_FALLBACK_WEB_PANEL";

fn main() {
    println!("cargo:rerun-if-changed=apps/web/out");
    println!("cargo:rerun-if-env-changed=AXON_ALLOW_FALLBACK_WEB_ASSETS");
    println!("cargo:rerun-if-env-changed=AXON_CONFIG_PATH");
    if let Some(path) = build_config_path() {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set"));
    let source = manifest_dir.join("apps/web/out");

    match asset_state(&source) {
        Ok(AssetState::Ready) => {}
        Ok(
            AssetState::Missing
            | AssetState::Empty
            | AssetState::Incomplete(_)
            | AssetState::FallbackOnly,
        ) if allow_fallback_assets() => {
            println!(
                "cargo:warning=apps/web/out is not a complete web build; embedding fallback web panel"
            );
            fs::create_dir_all(&source).expect("create web assets fallback dir");
            write_fallback_index(&source).expect("write fallback web index");
        }
        Ok(AssetState::Missing | AssetState::Empty) => {
            panic!("apps/web/out is empty; run the web build before compiling axon")
        }
        Ok(AssetState::FallbackOnly) => {
            panic!(
                "apps/web/out contains only fallback assets; run the web build before compiling axon"
            )
        }
        Ok(AssetState::Incomplete(reason)) => {
            panic!("apps/web/out is incomplete ({reason}); run the web build before compiling axon")
        }
        Err(error) => panic!("apps/web/out is unreadable: {error}"),
    }
}

fn allow_fallback_assets() -> bool {
    let explicit_config = env::var_os("AXON_CONFIG_PATH").map(PathBuf::from);
    let home = build_config_home();
    allow_fallback_assets_from(
        env::var("AXON_ALLOW_FALLBACK_WEB_ASSETS").ok().as_deref(),
        explicit_config.as_deref(),
        &home,
    )
}

fn build_config_home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn build_config_path() -> Option<PathBuf> {
    env::var_os("AXON_CONFIG_PATH")
        .map(PathBuf::from)
        .or_else(|| Some(build_config_home().join(".axon/config.toml")))
}

pub(crate) fn allow_fallback_assets_from(
    env_value: Option<&str>,
    explicit_config_path: Option<&Path>,
    home: &Path,
) -> bool {
    if env_value.is_some_and(|value| !value.trim().is_empty()) {
        return true;
    }

    let config_path = explicit_config_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".axon/config.toml"));
    read_build_allow_fallback_web_assets(&config_path).unwrap_or(false)
}

fn read_build_allow_fallback_web_assets(path: &Path) -> io::Result<bool> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };

    let mut in_build = false;
    for raw_line in contents.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_build = line == "[build]";
            continue;
        }
        if !in_build {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "allow-fallback-web-assets" {
            return Ok(matches!(value.trim(), "true" | "\"true\"" | "'true'"));
        }
    }
    Ok(false)
}

enum AssetState {
    Missing,
    Empty,
    Incomplete(&'static str),
    FallbackOnly,
    Ready,
}

fn asset_state(path: &Path) -> io::Result<AssetState> {
    if !path.is_dir() {
        return Ok(AssetState::Missing);
    }

    let entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    if entries.is_empty() {
        return Ok(AssetState::Empty);
    }

    let index_path = path.join("index.html");
    if !index_path.is_file() {
        return Ok(AssetState::Incomplete("missing index.html"));
    }

    let index = fs::read_to_string(index_path)?;
    if index.contains(FALLBACK_MARKER) {
        return Ok(AssetState::FallbackOnly);
    }

    if !path.join("_next").is_dir() && !path.join("assets").is_dir() {
        return Ok(AssetState::Incomplete("missing static asset directory"));
    }

    Ok(AssetState::Ready)
}

fn write_fallback_index(destination: &Path) -> io::Result<()> {
    fs::write(
        destination.join("index.html"),
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="axon-build" content="AXON_FALLBACK_WEB_PANEL" />
    <title>Axon</title>
  </head>
  <body>
    <main>
      <h1>Axon web panel is not built</h1>
      <p>Run <code>npm --prefix apps/web run build</code> to embed the full web panel.</p>
    </main>
  </body>
</html>
"#,
    )
}
