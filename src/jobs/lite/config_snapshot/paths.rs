use crate::core::config::Config;
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
mod tests {
    use super::normalize_container_output_dir;
    use crate::core::config::Config;
    use std::path::PathBuf;

    #[test]
    fn maps_default_host_axon_output_dir_for_container_workers() {
        let mut submitted = Config::test_default();
        submitted.output_dir = PathBuf::from("/home/jmagar/.axon/output");

        let mut worker = Config::test_default();
        worker.output_dir = PathBuf::from("/home/axon/.axon/output");

        normalize_container_output_dir(&worker, &mut submitted, true);

        assert_eq!(
            submitted.output_dir,
            PathBuf::from("/home/axon/.axon/output")
        );
    }

    #[test]
    fn keeps_non_default_axon_output_dir_for_container_workers() {
        let mut submitted = Config::test_default();
        submitted.output_dir = PathBuf::from("/mnt/shared/.axon/output");

        let mut worker = Config::test_default();
        worker.output_dir = PathBuf::from("/home/axon/.axon/output");

        normalize_container_output_dir(&worker, &mut submitted, true);

        assert_eq!(
            submitted.output_dir,
            PathBuf::from("/mnt/shared/.axon/output")
        );
    }
}
