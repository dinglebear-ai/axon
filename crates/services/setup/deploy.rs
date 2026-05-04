use super::assets::{DOCKER_COMPOSE_SERVICES, ENV_EXAMPLE};
use super::config_store::{validate_remote_dir, write_remote_service_urls};
use super::ssh_targets::list_ssh_targets;
use openssh::{KnownHosts, SessionBuilder};
use openssh_sftp_client::{Sftp, SftpOptions};
use serde::{Deserialize, Serialize};
use std::error::Error;

const DEFAULT_REMOTE_DIR: &str = "axon-deploy";

#[derive(Debug, Clone, Deserialize)]
pub struct DeployRequest {
    pub target: String,
    pub remote_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeployStep {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeployResult {
    pub target: String,
    pub remote_host: String,
    pub remote_dir: String,
    pub qdrant_url: String,
    pub tei_url: String,
    pub chrome_remote_url: String,
    pub config_path: String,
    pub steps: Vec<DeployStep>,
}

pub async fn deploy_remote(request: DeployRequest) -> Result<DeployResult, Box<dyn Error>> {
    let target = request.target.trim();
    if target.is_empty() {
        return Err("target is required".into());
    }
    let remote_dir =
        validate_remote_dir(request.remote_dir.as_deref().unwrap_or(DEFAULT_REMOTE_DIR))?;
    let remote_host = remote_host_for_target(target).unwrap_or_else(|| target.to_string());
    let mut steps = Vec::new();

    let session = connect(target).await?;
    run_checked(&session, "docker binary", "command -v docker").await?;
    steps.push(step("docker binary", "docker is installed"));
    run_checked(&session, "docker compose", "docker compose version").await?;
    steps.push(step("docker compose", "docker compose is available"));
    run_checked(&session, "docker daemon", "docker info >/dev/null").await?;
    steps.push(step("docker daemon", "docker daemon is reachable"));
    run_checked(
        &session,
        "remote directory",
        &format!("mkdir -p \"$HOME/{remote_dir}\""),
    )
    .await?;
    steps.push(step("remote directory", &format!("created ~/{remote_dir}")));
    session.close().await?;

    upload_assets(target, &remote_dir).await?;
    steps.push(step("upload assets", "compose and env templates uploaded"));

    let session = connect(target).await?;
    run_checked(
        &session,
        "compose up",
        &format!("docker compose -f \"$HOME/{remote_dir}/docker-compose.services.yaml\" up -d"),
    )
    .await?;
    steps.push(step("compose up", "remote services started"));
    session.close().await?;

    let qdrant_url = format!("http://{remote_host}:53333");
    let tei_url = format!("http://{remote_host}:52000");
    let chrome_remote_url = format!("http://{remote_host}:6000");
    let config_path = write_remote_service_urls(&qdrant_url, &tei_url, &chrome_remote_url)?;
    steps.push(step(
        "local config",
        &format!("updated {}", config_path.display()),
    ));

    Ok(DeployResult {
        target: target.to_string(),
        remote_host,
        remote_dir,
        qdrant_url,
        tei_url,
        chrome_remote_url,
        config_path: config_path.display().to_string(),
        steps,
    })
}

async fn connect(target: &str) -> Result<openssh::Session, openssh::Error> {
    let mut builder = SessionBuilder::default();
    builder.known_hosts_check(KnownHosts::Add);
    builder.connect(target).await
}

async fn upload_assets(target: &str, remote_dir: &str) -> Result<(), Box<dyn Error>> {
    let session = connect(target).await?;
    let sftp = Sftp::from_session(session, SftpOptions::default()).await?;
    let compose_path = format!("{remote_dir}/docker-compose.services.yaml");
    let env_path = format!("{remote_dir}/.env.example");
    write_sftp_file(&sftp, &compose_path, DOCKER_COMPOSE_SERVICES.as_bytes()).await?;
    write_sftp_file(&sftp, &env_path, ENV_EXAMPLE.as_bytes()).await?;
    sftp.close().await?;
    Ok(())
}

async fn write_sftp_file(sftp: &Sftp, path: &str, contents: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut file = sftp.create(path).await?;
    file.write_all(contents).await?;
    file.close().await?;
    Ok(())
}

async fn run_checked(
    session: &openssh::Session,
    label: &str,
    command: &str,
) -> Result<(), Box<dyn Error>> {
    let output = session
        .command("sh")
        .arg("-lc")
        .arg(command)
        .output()
        .await?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(format!(
        "{label} failed with status {}: {}{}",
        output.status,
        stdout.trim(),
        stderr.trim()
    )
    .into())
}

fn remote_host_for_target(target: &str) -> Option<String> {
    let targets = list_ssh_targets().ok()?;
    targets.into_iter().find_map(|ssh_target| {
        (ssh_target.alias == target).then(|| ssh_target.host_name.unwrap_or(ssh_target.alias))
    })
}

fn step(name: &str, detail: &str) -> DeployStep {
    DeployStep {
        name: name.to_string(),
        ok: true,
        detail: detail.to_string(),
    }
}
