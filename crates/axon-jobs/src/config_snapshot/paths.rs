use axon_core::config::Config;
use std::path::{Component, Path};

pub(super) fn normalize_container_output_dir(
    process_cfg: &Config,
    cfg: &mut Config,
    in_container: bool,
) {
    if !in_container {
        return;
    }
    if cfg.output_dir.starts_with(&process_cfg.output_dir) {
        return;
    }
    if !is_default_home_axon_output(&cfg.output_dir) {
        return;
    }
    cfg.output_dir = process_cfg.output_dir.clone();
}

fn is_default_home_axon_output(path: &Path) -> bool {
    let components: Vec<_> = path.components().collect();
    matches!(
        components.as_slice(),
        [
            Component::RootDir,
            Component::Normal(home),
            Component::Normal(_user),
            Component::Normal(axon),
            Component::Normal(output),
        ] if *home == "home" && *axon == ".axon" && *output == "output"
    )
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
