use std::fs;
use std::io;
use std::path::Path;

const FALLBACK_MARKER: &str = "AXON_FALLBACK_WEB_PANEL";

fn main() {
    println!("cargo:rerun-if-changed=../../apps/web/out");

    let manifest_dir = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set"),
    );
    let source = manifest_dir.join("../../apps/web/out");

    match asset_state(&source) {
        Ok(AssetState::Ready | AssetState::FallbackOnly) => {}
        Ok(AssetState::Missing | AssetState::Empty | AssetState::Incomplete) => {
            println!(
                "cargo:warning=apps/web/out is not a complete web build; embedding fallback web panel"
            );
            fs::create_dir_all(&source).expect("create web assets fallback dir");
            write_fallback_index(&source).expect("write fallback web index");
        }
        Err(error) => panic!("apps/web/out is unreadable: {error}"),
    }
}

enum AssetState {
    Missing,
    Empty,
    Incomplete,
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
        return Ok(AssetState::Incomplete);
    }

    let index = fs::read_to_string(index_path)?;
    if index.contains(FALLBACK_MARKER) {
        return Ok(AssetState::FallbackOnly);
    }

    if !path.join("_next").is_dir() && !path.join("assets").is_dir() {
        return Ok(AssetState::Incomplete);
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
