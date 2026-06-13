use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FALLBACK_MARKER: &str = "AXON_FALLBACK_WEB_PANEL";

fn main() {
    println!("cargo:rerun-if-changed=apps/web/out");
    println!("cargo:rerun-if-env-changed=AXON_ALLOW_FALLBACK_WEB_ASSETS");

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
    env::var_os("AXON_ALLOW_FALLBACK_WEB_ASSETS").is_some()
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
