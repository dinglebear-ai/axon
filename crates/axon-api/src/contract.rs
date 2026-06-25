//! Transport-neutral client/server contract DTOs.
//!
//! `ArtifactHandle` is the path-safe handle to a service-produced output file
//! (crawl markdown, screenshot, extract items). It is shared by the services
//! result types, the HTTP routes, and the jobs layer, so it lives here rather
//! than under services.

use percent_encoding::percent_decode_str;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct ArtifactHandle {
    kind: String,
    relative_path: String,
    display_path: String,
    bytes: u64,
    line_count: Option<u64>,
    job_id: Option<String>,
    url: Option<String>,
}

impl ArtifactHandle {
    pub fn try_new(
        kind: impl Into<String>,
        relative_path: impl Into<String>,
        display_path: impl Into<String>,
        bytes: u64,
        line_count: Option<u64>,
        job_id: Option<String>,
        url: Option<String>,
    ) -> Result<Self, String> {
        let relative_path = relative_path.into();
        if relative_path_is_unsafe(&relative_path) {
            return Err(format!("unsafe artifact relative_path: {relative_path}"));
        }
        Ok(Self::from_validated_parts(
            kind,
            relative_path,
            display_path,
            bytes,
            line_count,
            job_id,
            url,
        ))
    }

    pub fn new(
        kind: impl Into<String>,
        relative_path: impl Into<String>,
        display_path: impl Into<String>,
        bytes: u64,
        line_count: Option<u64>,
        job_id: Option<String>,
        url: Option<String>,
    ) -> Self {
        match Self::try_new(
            kind,
            relative_path,
            display_path,
            bytes,
            line_count,
            job_id,
            url,
        ) {
            Ok(handle) => handle,
            Err(message) => panic!("{message}"),
        }
    }

    pub fn try_from_path(
        kind: impl Into<String>,
        root: &Path,
        path: &Path,
        bytes: u64,
        line_count: Option<u64>,
        job_id: Option<String>,
        url: Option<String>,
    ) -> Option<Self> {
        if !path.is_absolute() || !root.is_absolute() {
            return None;
        }
        let relative_path = path
            .strip_prefix(root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        if relative_path_is_unsafe(&relative_path) {
            return None;
        }
        Some(Self::from_validated_parts(
            kind,
            relative_path,
            path.to_string_lossy().into_owned(),
            bytes,
            line_count,
            job_id,
            url,
        ))
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn display_path(&self) -> &str {
        &self.display_path
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn line_count(&self) -> Option<u64> {
        self.line_count
    }

    pub fn job_id(&self) -> Option<&str> {
        self.job_id.as_deref()
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    fn from_validated_parts(
        kind: impl Into<String>,
        relative_path: impl Into<String>,
        display_path: impl Into<String>,
        bytes: u64,
        line_count: Option<u64>,
        job_id: Option<String>,
        url: Option<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            relative_path: normalize_relative_path(relative_path.into()),
            display_path: display_path.into(),
            bytes,
            line_count,
            job_id,
            url,
        }
    }
}

impl<'de> Deserialize<'de> for ArtifactHandle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireArtifactHandle {
            kind: String,
            relative_path: String,
            display_path: String,
            bytes: u64,
            line_count: Option<u64>,
            job_id: Option<String>,
            url: Option<String>,
        }

        let wire = WireArtifactHandle::deserialize(deserializer)?;
        Self::try_new(
            wire.kind,
            wire.relative_path,
            wire.display_path,
            wire.bytes,
            wire.line_count,
            wire.job_id,
            wire.url,
        )
        .map_err(de::Error::custom)
    }
}

fn normalize_relative_path(path: String) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_string()
}

fn relative_path_is_unsafe(path: &str) -> bool {
    if path.is_empty() || path.contains('\0') || path.contains('\\') {
        return true;
    }
    let decoded = percent_decode_str(path).decode_utf8_lossy();
    if decoded.contains(':')
        || decoded.contains('\\')
        || decoded
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return true;
    }
    Path::new(decoded.as_ref()).components().any(|component| {
        matches!(
            component,
            std::path::Component::CurDir
                | std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    })
}
