use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=AXON_BUILD_GIT_SHA");
    let sha = std::env::var("AXON_BUILD_GIT_SHA").unwrap_or_else(|_| {
        let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default();
        Command::new("git")
            .args(["-C", &manifest_dir.to_string_lossy(), "rev-parse", "HEAD"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
    });
    println!("cargo:rustc-env=AXON_BUILD_GIT_SHA={sha}");
    println!(
        "cargo:rustc-env=AXON_BUILD_PROFILE={}",
        std::env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string())
    );
    println!("cargo:rustc-env=AXON_SCHEMA_EPOCH=1");
}
