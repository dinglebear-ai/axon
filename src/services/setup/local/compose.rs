use super::{LocalSetupPhase, LocalSetupStatus, PhaseTimer};
use crate::services::setup::assets;
use std::io;
use std::path::Path;

pub(super) fn write_compose_assets(compose_dir: &Path) -> io::Result<LocalSetupPhase> {
    let timer = PhaseTimer::start("compose-assets");
    std::fs::create_dir_all(compose_dir.join("config/chrome"))?;
    std::fs::create_dir_all(compose_dir.join("config/qdrant"))?;
    std::fs::write(
        compose_dir.join("docker-compose.yaml"),
        assets::DOCKER_COMPOSE_SERVICES,
    )?;
    std::fs::write(
        compose_dir.join("config/chrome/Dockerfile"),
        assets::CHROME_DOCKERFILE,
    )?;
    std::fs::write(
        compose_dir.join("config/qdrant/production.yaml"),
        assets::QDRANT_PRODUCTION_YAML,
    )?;
    Ok(timer.finish(
        LocalSetupStatus::Ok,
        format!("wrote compose assets under {}", compose_dir.display()),
    ))
}

pub(super) fn check_compose_assets(compose_dir: &Path) -> LocalSetupPhase {
    let timer = PhaseTimer::start("compose-assets");
    let compose = compose_dir.join("docker-compose.yaml");
    timer.finish(
        if compose.exists() {
            LocalSetupStatus::Ok
        } else {
            LocalSetupStatus::Warn
        },
        if compose.exists() {
            format!("found {}", compose.display())
        } else {
            format!("missing {}; run axon setup", compose.display())
        },
    )
}
