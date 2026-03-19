use crate::crates::core::config::parse::normalize_local_service_url;
use crate::crates::core::config::{Config, McpTransport};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn parse_origin_allowlist(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub fn load_mcp_config() -> Config {
    let mut cfg = Config::default();

    if let Some(v) = env("AXON_PG_URL") {
        cfg.pg_url = normalize_local_service_url(v);
    }
    if let Some(v) = env("AXON_REDIS_URL") {
        cfg.redis_url = normalize_local_service_url(v);
    }
    if let Some(v) = env("AXON_AMQP_URL") {
        cfg.amqp_url = normalize_local_service_url(v);
    }
    if let Some(v) = env("QDRANT_URL") {
        cfg.qdrant_url = normalize_local_service_url(v);
    }
    if let Some(v) = env("TEI_URL") {
        cfg.tei_url = normalize_local_service_url(v);
    }
    if let Some(v) = env("OPENAI_BASE_URL") {
        cfg.openai_base_url = v;
    }
    if let Some(v) = env("OPENAI_API_KEY") {
        cfg.openai_api_key = v;
    }
    if let Some(v) = env("OPENAI_MODEL") {
        cfg.openai_model = v;
    }
    if let Some(v) = env("TAVILY_API_KEY") {
        cfg.tavily_api_key = v;
    }
    if let Some(v) = env("AXON_WEB_ALLOWED_ORIGINS") {
        cfg.web_allowed_origins = parse_origin_allowlist(&v);
    }
    if let Some(v) = env("AXON_SHELL_ALLOWED_ORIGINS") {
        cfg.shell_allowed_origins = parse_origin_allowlist(&v);
    }

    if let Some(v) = env("AXON_COLLECTION") {
        cfg.collection = v;
    }
    if let Some(v) = env("AXON_CRAWL_QUEUE") {
        cfg.crawl_queue = v;
    }
    if let Some(v) = env("AXON_EXTRACT_QUEUE") {
        cfg.extract_queue = v;
    }
    if let Some(v) = env("AXON_EMBED_QUEUE") {
        cfg.embed_queue = v;
    }
    if let Some(v) = env("AXON_INGEST_QUEUE") {
        cfg.ingest_queue = v;
    }
    if let Some(v) = env("AXON_REFRESH_QUEUE") {
        cfg.refresh_queue = v;
    }

    // Ask authoritative tuning
    if let Some(v) = env("AXON_ASK_AUTHORITATIVE_DOMAINS") {
        cfg.ask_authoritative_domains = v
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
    }
    if let Some(v) = env("AXON_ASK_AUTHORITATIVE_BOOST")
        && let Ok(f) = v.parse::<f64>()
    {
        cfg.ask_authoritative_boost = f.clamp(0.0, 0.5);
    }
    if let Some(v) = env("AXON_ASK_AUTHORITATIVE_ALLOWLIST") {
        cfg.ask_authoritative_allowlist = v
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
    }
    if let Some(v) = env("AXON_ASK_MIN_CITATIONS_NONTRIVIAL")
        && let Ok(n) = v.parse::<usize>()
    {
        cfg.ask_min_citations_nontrivial = n.clamp(1, 5);
    }

    if let Some(v) = env("AXON_CHROME_REMOTE_URL") {
        cfg.chrome_remote_url = Some(normalize_local_service_url(v));
    }

    if let Some(v) = env("AXON_MCP_HTTP_HOST") {
        cfg.mcp_http_host = v;
    }
    if let Some(v) = env("AXON_MCP_HTTP_PORT")
        && let Ok(port) = v.parse::<u16>()
    {
        cfg.mcp_http_port = port;
    }
    if let Some(v) = env("AXON_MCP_TRANSPORT") {
        cfg.mcp_transport = match v.as_str() {
            "stdio" => McpTransport::Stdio,
            "both" => McpTransport::Both,
            _ => McpTransport::Http,
        };
    }

    if let Some(v) = env("GITHUB_TOKEN") {
        cfg.github_token = Some(v);
    }
    if let Some(v) = env("REDDIT_CLIENT_ID") {
        cfg.reddit_client_id = Some(v);
    }
    if let Some(v) = env("REDDIT_CLIENT_SECRET") {
        cfg.reddit_client_secret = Some(v);
    }

    cfg.json_output = true;
    cfg.wait = false;
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[allow(unsafe_code)]
    #[test]
    fn load_mcp_config_reads_origin_allowlists() {
        let _guard = ENV_LOCK.lock().unwrap();
        const WEB: &str = "AXON_WEB_ALLOWED_ORIGINS";
        const SHELL: &str = "AXON_SHELL_ALLOWED_ORIGINS";
        let prev_web = env::var(WEB).ok();
        let prev_shell = env::var(SHELL).ok();

        unsafe {
            env::set_var(WEB, "https://axon.example.com,http://localhost:49010");
            env::set_var(SHELL, "http://localhost:49011");
        }

        let cfg = load_mcp_config();

        assert_eq!(
            cfg.web_allowed_origins,
            vec![
                "https://axon.example.com".to_string(),
                "http://localhost:49010".to_string(),
            ]
        );
        assert_eq!(
            cfg.shell_allowed_origins,
            vec!["http://localhost:49011".to_string()]
        );

        match prev_web {
            Some(v) => unsafe { env::set_var(WEB, v) },
            None => unsafe { env::remove_var(WEB) },
        }
        match prev_shell {
            Some(v) => unsafe { env::set_var(SHELL, v) },
            None => unsafe { env::remove_var(SHELL) },
        }
    }

    #[allow(unsafe_code)]
    #[test]
    fn load_mcp_config_normalizes_tei_url() {
        let _guard = ENV_LOCK.lock().unwrap();
        const TEI: &str = "TEI_URL";
        let prev_tei = env::var(TEI).ok();

        unsafe {
            env::set_var(TEI, "http://axon-tei:80");
        }

        let cfg = load_mcp_config();

        assert_eq!(
            cfg.tei_url,
            normalize_local_service_url("http://axon-tei:80".to_string())
        );

        match prev_tei {
            Some(v) => unsafe { env::set_var(TEI, v) },
            None => unsafe { env::remove_var(TEI) },
        }
    }
}
