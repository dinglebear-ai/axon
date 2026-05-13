use super::assets::{
    CHROME_DOCKERFILE, DOCKER_COMPOSE_SERVICES, ENV_EXAMPLE, QDRANT_PRODUCTION_YAML, SERVICES_ENV,
};
use super::config_store::{validate_remote_dir, write_remote_runtime_env};
use super::ssh_targets::list_ssh_targets;
use crate::core::paths::axon_home_dir;
use openssh::{KnownHosts, SessionBuilder};
use openssh_sftp_client::{Sftp, SftpOptions};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::future::Future;
use std::time::Duration;
use tokio::time::timeout;

const DEFAULT_REMOTE_DIR: &str = "axon-deploy";
const SSH_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const REMOTE_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
const SFTP_TIMEOUT: Duration = Duration::from_secs(120);
const COMPOSE_TIMEOUT: Duration = Duration::from_secs(300);
const READINESS_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Deserialize)]
pub struct DeployRequest {
    pub target: String,
    pub remote_dir: Option<String>,
    #[serde(default)]
    pub public_exposure: Option<bool>,
    #[serde(default)]
    pub accept_new_host_key: Option<bool>,
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
    pub public_exposure: bool,
    pub qdrant_url: String,
    pub tei_url: String,
    pub chrome_remote_url: String,
    pub runtime_env_path: String,
    pub tunnel_command: Option<String>,
    pub steps: Vec<DeployStep>,
}

pub async fn deploy_remote(request: DeployRequest) -> Result<DeployResult, Box<dyn Error>> {
    let target = request.target.trim();
    if target.is_empty() {
        return Err("target is required".into());
    }
    let remote_dir =
        validate_remote_dir(request.remote_dir.as_deref().unwrap_or(DEFAULT_REMOTE_DIR))?;
    let public_exposure = request.public_exposure.unwrap_or(false);
    if public_exposure {
        return Err(
            "--public-exposure is disabled for Axon infra services; use the SSH tunnel output or put Qdrant/TEI/Chrome behind an authenticated proxy"
                .into(),
        );
    }
    let accept_new_host_key = request.accept_new_host_key.unwrap_or(false);
    let remote_host = remote_host_for_target(target).unwrap_or_else(|| target.to_string());
    let mut steps = Vec::new();

    let session = connect(target, accept_new_host_key).await?;
    run_checked(&session, "docker binary", "command -v docker").await?;
    steps.push(step("docker binary", "docker is installed"));
    run_checked(&session, "curl binary", "command -v curl").await?;
    steps.push(step("curl binary", "curl is installed"));
    run_checked(&session, "docker compose", "docker compose version").await?;
    steps.push(step("docker compose", "docker compose is available"));
    run_checked(&session, "docker daemon", "docker info >/dev/null").await?;
    steps.push(step("docker daemon", "docker daemon is reachable"));
    run_checked(
        &session,
        "remote directory",
        &format!("mkdir -p \"$HOME/{remote_dir}/qdrant\" \"$HOME/{remote_dir}/chrome\""),
    )
    .await?;
    steps.push(step("remote directory", &format!("created ~/{remote_dir}")));
    session.close().await?;

    upload_assets(target, &remote_dir, public_exposure, accept_new_host_key).await?;
    steps.push(step("upload assets", "complete compose project uploaded"));

    let session = connect(target, accept_new_host_key).await?;
    run_checked_with_timeout(
        &session,
        "compose up",
        &format!("docker compose -f \"$HOME/{remote_dir}/docker-compose.services.yaml\" up -d"),
        COMPOSE_TIMEOUT,
    )
    .await?;
    steps.push(step("compose up", "remote services started"));
    wait_for_remote_services(&session).await?;
    steps.push(step(
        "service readiness",
        "qdrant, tei, and chrome are ready",
    ));
    session.close().await?;

    let (qdrant_url, tei_url, chrome_remote_url, tunnel_command) =
        service_urls(target, &remote_host, public_exposure);
    let axon_home =
        axon_home_dir().ok_or("HOME is unset or invalid; cannot update ~/.axon/.env")?;
    let env_path = effective_env_path(&axon_home.join(".env"))?;
    let runtime_env_path =
        write_remote_runtime_env(&env_path, &qdrant_url, &tei_url, &chrome_remote_url)?;
    steps.push(step(
        "local env",
        &format!("updated {}", runtime_env_path.display()),
    ));

    Ok(DeployResult {
        target: target.to_string(),
        remote_host,
        remote_dir,
        public_exposure,
        qdrant_url,
        tei_url,
        chrome_remote_url,
        runtime_env_path: runtime_env_path.display().to_string(),
        tunnel_command,
        steps,
    })
}

async fn connect(
    target: &str,
    accept_new_host_key: bool,
) -> Result<openssh::Session, Box<dyn Error>> {
    let mut builder = SessionBuilder::default();
    builder.known_hosts_check(host_key_policy(accept_new_host_key));
    with_timeout("ssh connect", SSH_CONNECT_TIMEOUT, builder.connect(target)).await
}

fn host_key_policy(accept_new_host_key: bool) -> KnownHosts {
    if accept_new_host_key {
        KnownHosts::Add
    } else {
        KnownHosts::Strict
    }
}

async fn upload_assets(
    target: &str,
    remote_dir: &str,
    public_exposure: bool,
    accept_new_host_key: bool,
) -> Result<(), Box<dyn Error>> {
    let session = connect(target, accept_new_host_key).await?;
    let sftp = with_timeout(
        "sftp connect",
        SFTP_TIMEOUT,
        Sftp::from_session(session, SftpOptions::default()),
    )
    .await?;
    let compose_path = format!("{remote_dir}/docker-compose.services.yaml");
    let env_path = format!("{remote_dir}/.env.example");
    let services_env_path = format!("{remote_dir}/services.env");
    let qdrant_path = format!("{remote_dir}/qdrant/production.yaml");
    let chrome_path = format!("{remote_dir}/chrome/Dockerfile");
    let compose = render_compose(public_exposure);
    for (path, contents) in [
        (compose_path.as_str(), compose.as_bytes()),
        (env_path.as_str(), ENV_EXAMPLE.as_bytes()),
        (services_env_path.as_str(), SERVICES_ENV.as_bytes()),
        (qdrant_path.as_str(), QDRANT_PRODUCTION_YAML.as_bytes()),
        (chrome_path.as_str(), CHROME_DOCKERFILE.as_bytes()),
    ] {
        write_sftp_file(&sftp, path, contents).await?;
    }
    with_timeout("sftp close", SFTP_TIMEOUT, sftp.close()).await?;
    Ok(())
}

async fn write_sftp_file(sftp: &Sftp, path: &str, contents: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut file = with_timeout("sftp create", SFTP_TIMEOUT, sftp.create(path)).await?;
    with_timeout("sftp write", SFTP_TIMEOUT, file.write_all(contents)).await?;
    with_timeout("sftp file close", SFTP_TIMEOUT, file.close()).await?;
    Ok(())
}

async fn run_checked(
    session: &openssh::Session,
    label: &str,
    command: &str,
) -> Result<(), Box<dyn Error>> {
    run_checked_with_timeout(session, label, command, REMOTE_COMMAND_TIMEOUT).await
}

async fn run_checked_with_timeout(
    session: &openssh::Session,
    label: &str,
    command: &str,
    duration: Duration,
) -> Result<(), Box<dyn Error>> {
    let output = with_timeout(
        label,
        duration,
        session.command("sh").arg("-lc").arg(command).output(),
    )
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

async fn wait_for_remote_services(session: &openssh::Session) -> Result<(), Box<dyn Error>> {
    for (label, url) in [
        ("qdrant readiness", "http://127.0.0.1:53333/readyz"),
        ("tei readiness", "http://127.0.0.1:52000/health"),
        ("chrome readiness", "http://127.0.0.1:6000/json/version"),
    ] {
        run_checked_with_timeout(
            session,
            label,
            &format!(
                "deadline=$((SECONDS+170)); \
                 until curl -fsS --max-time 5 {url} >/dev/null; do \
                   if [ \"$SECONDS\" -ge \"$deadline\" ]; then exit 1; fi; \
                   sleep 2; \
                 done"
            ),
            READINESS_TIMEOUT,
        )
        .await?;
    }
    Ok(())
}

async fn with_timeout<T, E, F>(
    label: &str,
    duration: Duration,
    future: F,
) -> Result<T, Box<dyn Error>>
where
    E: Error + 'static,
    F: Future<Output = Result<T, E>>,
{
    timeout(duration, future)
        .await
        .map_err(|_| format!("{label} timed out after {}s", duration.as_secs()))?
        .map_err(|err| Box::new(err) as Box<dyn Error>)
}

fn render_compose(_public_exposure: bool) -> String {
    DOCKER_COMPOSE_SERVICES.replace("../services.env", "services.env")
}

fn service_urls(
    target: &str,
    _remote_host: &str,
    _public_exposure: bool,
) -> (String, String, String, Option<String>) {
    (
        "http://127.0.0.1:53333".to_string(),
        "http://127.0.0.1:52000".to_string(),
        "http://127.0.0.1:6000".to_string(),
        Some(format!(
            "ssh -N -L 53333:127.0.0.1:53333 -L 52000:127.0.0.1:52000 -L 6000:127.0.0.1:6000 {target}"
        )),
    )
}

fn effective_env_path(
    default_path: &std::path::Path,
) -> Result<std::path::PathBuf, Box<dyn Error>> {
    let Ok(explicit) = std::env::var("AXON_ENV_FILE") else {
        return Ok(default_path.to_path_buf());
    };
    let trimmed = explicit.trim();
    if trimmed.is_empty() {
        return Ok(default_path.to_path_buf());
    }
    Ok(std::path::PathBuf::from(trimmed))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_rendering_is_private_even_when_public_requested() {
        let private = render_compose(false);
        assert!(private.contains("127.0.0.1:53333:6333"));
        assert!(private.contains("127.0.0.1:${TEI_HTTP_PORT:-52000}:80"));
        assert!(private.contains("127.0.0.1:6000:6000"));

        let public = render_compose(true);
        assert!(public.contains("127.0.0.1:53333:6333"));
        assert!(public.contains("127.0.0.1:${TEI_HTTP_PORT:-52000}:80"));
        assert!(public.contains("127.0.0.1:6000:6000"));
    }

    #[test]
    fn private_service_urls_use_local_tunnel_endpoints() {
        let (qdrant, tei, chrome, tunnel) = service_urls("prod", "prod.example.test", false);
        assert_eq!(qdrant, "http://127.0.0.1:53333");
        assert_eq!(tei, "http://127.0.0.1:52000");
        assert_eq!(chrome, "http://127.0.0.1:6000");
        assert_eq!(
            tunnel.unwrap(),
            "ssh -N -L 53333:127.0.0.1:53333 -L 52000:127.0.0.1:52000 -L 6000:127.0.0.1:6000 prod"
        );
    }

    #[test]
    fn public_service_urls_stay_tunneled() {
        let (qdrant, tei, chrome, tunnel) = service_urls("prod", "prod.example.test", true);
        assert_eq!(qdrant, "http://127.0.0.1:53333");
        assert_eq!(tei, "http://127.0.0.1:52000");
        assert_eq!(chrome, "http://127.0.0.1:6000");
        assert!(tunnel.is_some());
    }

    #[test]
    fn host_key_policy_is_strict_unless_explicitly_accepted() {
        assert!(matches!(host_key_policy(false), KnownHosts::Strict));
        assert!(matches!(host_key_policy(true), KnownHosts::Add));
    }

    #[test]
    fn bundled_assets_cover_compose_references() {
        assert!(!SERVICES_ENV.trim().is_empty());
        assert!(DOCKER_COMPOSE_SERVICES.contains("./config/qdrant/production.yaml"));
        assert!(QDRANT_PRODUCTION_YAML.contains("http_port: 6333"));
        assert!(DOCKER_COMPOSE_SERVICES.contains("chrome/Dockerfile"));
        assert!(CHROME_DOCKERFILE.contains("FROM "));
    }
}
