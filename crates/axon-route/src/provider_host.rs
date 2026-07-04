pub(crate) fn is_youtube_host(host: &str) -> bool {
    host == "youtube.com" || host.ends_with(".youtube.com")
}

pub(crate) fn is_github_host(host: &str) -> bool {
    host == "github.com" || host.ends_with(".github.com")
}

pub(crate) fn is_gitlab_host(host: &str) -> bool {
    host == "gitlab.com" || host.ends_with(".gitlab.com") || host.starts_with("gitlab.")
}

pub(crate) fn is_gitea_host(host: &str) -> bool {
    host == "codeberg.org"
        || host.ends_with(".codeberg.org")
        || host == "gitea.com"
        || host.ends_with(".gitea.com")
        || host == "forgejo.org"
        || host.ends_with(".forgejo.org")
        || host.starts_with("gitea.")
        || host.starts_with("forgejo.")
}
