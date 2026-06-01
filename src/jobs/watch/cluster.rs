//! Group changed URLs into crawl clusters by shared directory ancestry. Pure.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cluster {
    pub seed: String,
    pub members: Vec<String>,
}

fn parts(url: &str) -> Option<(String, Vec<String>)> {
    let (scheme, rest) = url.split_once("://")?;
    let (host, path) = match rest.split_once('/') {
        Some((h, p)) => (h, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    if host.is_empty() {
        return None;
    }
    // Strip the query string and fragment so `?` / `#` don't leak into the last
    // path segment and skew prefix grouping (e.g. `/docs/a?v=2` vs `/docs/a`).
    let path = path.split(['?', '#']).next().unwrap_or(&path).to_string();
    let host_key = format!("{scheme}://{host}");
    let mut segs: Vec<String> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    if !path.ends_with('/') && !segs.is_empty() {
        segs.pop();
    }
    Some((host_key, segs))
}

fn common_prefix(a: &[String], b: &[String]) -> Vec<String> {
    a.iter()
        .zip(b.iter())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x.clone())
        .collect()
}

pub fn group_by_common_prefix(urls: &[String]) -> Vec<Cluster> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, (Vec<String>, Vec<String>)> =
        std::collections::HashMap::new();
    for (idx, url) in urls.iter().enumerate() {
        let (key, dir, member) = match parts(url) {
            Some((host_key, segs)) if !segs.is_empty() => {
                (format!("{host_key}|{}", segs[0]), segs, url.clone())
            }
            _ => (format!("__solo_{idx}"), Vec::new(), url.clone()),
        };
        let entry = groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            (dir.clone(), Vec::new())
        });
        entry.0 = if entry.1.is_empty() {
            dir
        } else {
            common_prefix(&entry.0, &dir)
        };
        entry.1.push(member);
    }
    order
        .into_iter()
        .map(|key| {
            let (prefix, members) = groups.remove(&key).expect("key");
            let seed = if members.len() == 1 {
                members[0].clone()
            } else {
                let (host_key, _) = parts(&members[0]).expect("member parses");
                if prefix.is_empty() {
                    format!("{host_key}/")
                } else {
                    format!("{host_key}/{}/", prefix.join("/"))
                }
            };
            Cluster { seed, members }
        })
        .collect()
}

#[cfg(test)]
#[path = "cluster_tests.rs"]
mod tests;
