use crate::core::config::Config;

pub(crate) fn normalize_container_output_dir(
    process_cfg: &Config,
    cfg: &mut Config,
    in_container: bool,
) {
    if !in_container {
        return;
    }
    if cfg.output_dir == process_cfg.output_dir
        || cfg.output_dir.starts_with(&process_cfg.output_dir)
    {
        return;
    }
    if !cfg
        .output_dir
        .ends_with(std::path::Path::new(".axon/output"))
    {
        return;
    }
    cfg.output_dir = process_cfg.output_dir.clone();
}
