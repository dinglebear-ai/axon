//! Canonical URL validation for skills.sh catalog entries.

use url::Url;

use super::SkillsShSkill;

pub(super) fn canonical_install_url(skill: &SkillsShSkill) -> Option<String> {
    let install_url = skill.install_url.as_deref()?;
    let mut url = Url::parse(install_url).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return None;
    }
    match skill.source_type.as_str() {
        "github" => {
            if !valid_repository_name(&skill.source)
                || !url
                    .host_str()
                    .is_some_and(|host| host.eq_ignore_ascii_case("github.com"))
            {
                return None;
            }
            let path = url.path().trim_end_matches('/');
            let path = path.strip_suffix(".git").unwrap_or(path);
            if path != format!("/{}", skill.source) {
                return None;
            }
            url.set_path(&format!("/{}", skill.source));
        }
        "well-known" => {
            if !valid_well_known_source(&skill.source)
                || !url
                    .host_str()
                    .is_some_and(|host| host.eq_ignore_ascii_case(&skill.source))
            {
                return None;
            }
        }
        _ => return None,
    }
    let _ = url.set_port(None);
    url.set_query(None);
    url.set_fragment(None);
    Some(url.as_str().trim_end_matches('/').to_string())
}

pub(super) fn canonical_skills_sh_page(skill: &SkillsShSkill) -> String {
    if let Some(raw) = skill.url.as_deref()
        && let Ok(mut url) = Url::parse(raw)
        && url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case("skills.sh"))
        && url.path().trim_end_matches('/') == format!("/{}", skill.id)
    {
        let _ = url.set_port(None);
        url.set_query(None);
        url.set_fragment(None);
        return url.as_str().trim_end_matches('/').to_string();
    }
    let Ok(mut url) = Url::parse("https://skills.sh/") else {
        return "https://skills.sh/".to_string();
    };
    if let Ok(mut path) = url.path_segments_mut() {
        path.clear();
        for segment in skill.id.split('/').filter(|segment| !segment.is_empty()) {
            path.push(segment);
        }
    }
    url.as_str().trim_end_matches('/').to_string()
}

fn valid_repository_name(value: &str) -> bool {
    let mut parts = value.split('/');
    matches!((parts.next(), parts.next(), parts.next()), (Some(owner), Some(repo), None) if valid_source_segment(owner) && valid_source_segment(repo))
}

fn valid_well_known_source(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && !value.contains('/')
        && !value.chars().any(char::is_control)
        && Url::parse(&format!("https://{value}/"))
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .is_some_and(|host| host.eq_ignore_ascii_case(value))
}

fn valid_source_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !matches!(value, "." | "..")
        && !value
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | '?' | '#'))
}
