//! GitHub release discovery and asset download helpers.

use super::{GithubRelease, ReleaseAssetNames, err, version_tuple};
use futures_util::StreamExt;
use std::error::Error;
use std::path::Path;
use tokio::io::AsyncWriteExt;

const MAX_UPDATE_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;

pub(super) async fn download_release_assets(
    client: &reqwest::Client,
    release: &GithubRelease,
    names: &ReleaseAssetNames,
    archive_path: &Path,
) -> Result<(String, String), Box<dyn Error>> {
    let archive_url = find_asset_url(release, names.archive)?;
    let checksum_url = find_asset_url(release, names.checksum)?;
    download_to_file(client, archive_url, archive_path).await?;
    let checksum_response = client.get(checksum_url).send().await?.error_for_status()?;
    let checksum =
        axon_core::http::read_response_text_bounded(checksum_response, 1024 * 1024).await?;

    Ok((release.tag_name.clone(), checksum))
}

pub(super) async fn fetch_compatible_release(
    client: &reqwest::Client,
    repo: &str,
    version: Option<&str>,
    names: &ReleaseAssetNames,
) -> Result<GithubRelease, Box<dyn Error>> {
    if let Some(tag) = version {
        let response = client
            .get(format!(
                "https://api.github.com/repos/{repo}/releases/tags/{tag}"
            ))
            .send()
            .await?
            .error_for_status()?;
        let release: GithubRelease = axon_core::http::read_response_json_bounded(
            response,
            axon_core::http::DEFAULT_MAX_RESPONSE_BODY_BYTES,
        )
        .await?;
        find_asset_url(&release, names.archive)?;
        find_asset_url(&release, names.checksum)?;
        return Ok(release);
    }

    let response = client
        .get(format!(
            "https://api.github.com/repos/{repo}/releases?per_page=100"
        ))
        .send()
        .await?
        .error_for_status()?;
    let releases: Vec<GithubRelease> = axon_core::http::read_response_json_bounded(
        response,
        axon_core::http::DEFAULT_MAX_RESPONSE_BODY_BYTES,
    )
    .await?;
    select_latest_compatible_release(&releases, names).cloned()
}

pub(super) fn select_latest_compatible_release<'a>(
    releases: &'a [GithubRelease],
    names: &ReleaseAssetNames,
) -> Result<&'a GithubRelease, Box<dyn Error>> {
    releases
        .iter()
        .filter(|release| {
            !release.draft
                && !release.prerelease
                && release
                    .assets
                    .iter()
                    .any(|asset| asset.name == names.archive)
                && release
                    .assets
                    .iter()
                    .any(|asset| asset.name == names.checksum)
        })
        .filter_map(|release| {
            version_tuple(&release.tag_name)
                .ok()
                .map(|version| (version, release))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, release)| release)
        .ok_or_else(|| {
            err(format!(
                "no stable semantic-versioned release contains both {} and {}",
                names.archive, names.checksum
            ))
        })
}

pub(super) async fn download_to_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
) -> Result<(), Box<dyn Error>> {
    download_to_file_bounded(client, url, dest, MAX_UPDATE_DOWNLOAD_BYTES).await
}

async fn download_to_file_bounded(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    max_bytes: u64,
) -> Result<(), Box<dyn Error>> {
    let response = client.get(url).send().await?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes)
    {
        return Err(err(format!(
            "update download exceeds {max_bytes} byte limit"
        )));
    }
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = response.bytes_stream();
    let mut total = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        total = total.saturating_add(chunk.len() as u64);
        if total > max_bytes {
            drop(file);
            let _ = tokio::fs::remove_file(dest).await;
            return Err(err(format!(
                "update download exceeds {max_bytes} byte limit"
            )));
        }
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

pub(super) fn find_asset_url<'a>(
    release: &'a GithubRelease,
    name: &str,
) -> Result<&'a str, Box<dyn Error>> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .map(|asset| asset.browser_download_url.as_str())
        .ok_or_else(|| {
            err(format!(
                "release {} is missing asset {name}",
                release.tag_name
            ))
        })
}

pub(super) fn release_asset_names(
    os: &str,
    arch: &str,
) -> Result<ReleaseAssetNames, Box<dyn Error>> {
    match (os, arch) {
        ("linux", "x86_64") => Ok(ReleaseAssetNames {
            archive: "axon-linux-x86_64.tar.gz",
            checksum: "axon-linux-x86_64.tar.gz.sha256",
            signature: "axon-linux-x86_64.tar.gz.minisig",
        }),
        _ => Err(err(format!(
            "unsupported platform for axon update: {os}/{arch}; only linux/x86_64 is wired"
        ))),
    }
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
