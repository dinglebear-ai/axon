//! GitHub release discovery and asset download helpers.

use super::{GithubRelease, ReleaseAssetNames, err, version_tuple};
use futures_util::StreamExt;
use std::error::Error;
use std::path::Path;
use tokio::io::AsyncWriteExt;

pub(super) async fn download_release_assets(
    client: &reqwest::Client,
    release: &GithubRelease,
    names: &ReleaseAssetNames,
    archive_path: &Path,
) -> Result<(String, String), Box<dyn Error>> {
    let archive_url = find_asset_url(release, names.archive)?;
    let checksum_url = find_asset_url(release, names.checksum)?;
    download_to_file(client, archive_url, archive_path).await?;
    let checksum = client
        .get(checksum_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    Ok((release.tag_name.clone(), checksum))
}

pub(super) async fn fetch_compatible_release(
    client: &reqwest::Client,
    repo: &str,
    version: Option<&str>,
    names: &ReleaseAssetNames,
) -> Result<GithubRelease, Box<dyn Error>> {
    if let Some(tag) = version {
        let release: GithubRelease = client
            .get(format!(
                "https://api.github.com/repos/{repo}/releases/tags/{tag}"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        find_asset_url(&release, names.archive)?;
        find_asset_url(&release, names.checksum)?;
        return Ok(release);
    }

    let releases: Vec<GithubRelease> = client
        .get(format!(
            "https://api.github.com/repos/{repo}/releases?per_page=100"
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
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
    let response = client.get(url).send().await?.error_for_status()?;
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk?).await?;
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
