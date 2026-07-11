use axon_core::paths::{axon_data_base_dir, ensure_private_dir_async};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::sync::LazyLock;
use tokio::sync::Mutex;
use utoipa::ToSchema;

static STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
const STORE_KEY_SEPARATOR: char = '\u{1f}';

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MobileChatItem {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub payload: serde_json::Value,
    pub timestamp: i64,
}

/// Mobile session lifecycle/sync status.
///
/// Contract: `docs/pipeline-unification/surfaces/android-contract.md`
/// ("Mobile Session Model" -- `active`, `archived`, `deleted`, or
/// `sync_conflict`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MobileSessionStatus {
    /// Normal, in-use session.
    #[default]
    Active,
    /// Hidden from the default list but retained.
    Archived,
    /// Soft-deleted; retained briefly for sync propagation.
    Deleted,
    /// A concurrent update was rejected and the client must reconcile.
    SyncConflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MobileSession {
    pub id: String,
    pub title: String,
    pub first_message_preview: String,
    pub turn_count: u32,
    pub injected_op_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<i64>,
    #[serde(default)]
    pub items: Vec<MobileChatItem>,
    #[serde(default)]
    pub status: MobileSessionStatus,
    /// Sources/jobs/artifacts linked to this session (ids or URLs).
    #[serde(default)]
    pub source_refs: Vec<String>,
    /// Optional redacted/encrypted draft payload the client is composing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    /// Optimistic concurrency token. Clients must echo back the version they
    /// last observed; the server increments it on every successful upsert and
    /// rejects mismatched versions with a stale-update conflict.
    #[serde(default)]
    pub sync_version: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MobileSessionSummary {
    pub id: String,
    pub title: String,
    pub first_message_preview: String,
    pub turn_count: u32,
    pub injected_op_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MobileSessionListResponse {
    pub sessions: Vec<MobileSessionSummary>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MobileSessionDetailResponse {
    pub session: MobileSession,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpsertMobileSessionRequest {
    pub session: MobileSession,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UpsertMobileSessionResponse {
    pub ok: bool,
    pub session: MobileSession,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DeleteMobileSessionResponse {
    pub ok: bool,
}

#[derive(Debug)]
pub enum MobileSessionError {
    InvalidId,
    InvalidSession(String),
    IdMismatch,
    NotFound,
    StaleUpdate,
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl Display for MobileSessionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId => write!(
                f,
                "session id must be 1-128 ASCII letters, numbers, '-' or '_'"
            ),
            Self::InvalidSession(message) => write!(f, "invalid mobile session: {message}"),
            Self::IdMismatch => write!(f, "path session id does not match request body session id"),
            Self::NotFound => write!(f, "mobile session not found"),
            Self::StaleUpdate => write!(f, "mobile session update is older than stored state"),
            Self::Io(err) => write!(f, "{err}"),
            Self::Json(err) => write!(f, "{err}"),
        }
    }
}

impl Error for MobileSessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Json(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for MobileSessionError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for MobileSessionError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

type Result<T> = std::result::Result<T, MobileSessionError>;

pub async fn list_sessions(owner: &str) -> Result<MobileSessionListResponse> {
    let _guard = STORE_LOCK.lock().await;
    let mut store = read_store().await?;
    if migrate_legacy_entries(&mut store, owner) {
        write_store(&store).await?;
    }
    let mut sessions: Vec<_> = store
        .iter()
        .filter(|(key, _session)| store_key_owner_matches(key, owner))
        .map(|(_key, session)| summary(session))
        .collect();
    sort_summaries(&mut sessions);
    Ok(MobileSessionListResponse { sessions })
}

pub async fn get_session(owner: &str, id: &str) -> Result<MobileSessionDetailResponse> {
    validate_id(id)?;
    let _guard = STORE_LOCK.lock().await;
    let mut store = read_store().await?;
    if migrate_legacy_entries(&mut store, owner) {
        write_store(&store).await?;
    }
    let session = store
        .get(&store_key(owner, id))
        .cloned()
        .ok_or(MobileSessionError::NotFound)?;
    Ok(MobileSessionDetailResponse { session })
}

pub async fn upsert_session(
    owner: &str,
    path_id: &str,
    request: UpsertMobileSessionRequest,
) -> Result<UpsertMobileSessionResponse> {
    validate_id(path_id)?;
    validate_id(&request.session.id)?;
    if path_id != request.session.id {
        return Err(MobileSessionError::IdMismatch);
    }
    let _guard = STORE_LOCK.lock().await;
    let mut store = read_store().await?;
    migrate_legacy_entries(&mut store, owner);
    upsert_into_store(&mut store, owner, path_id, request.session)?;
    // Read back the persisted state (not the request body) so the response
    // reflects the server-incremented `sync_version`, not the client's guess.
    let session = store
        .get(&store_key(owner, path_id))
        .cloned()
        .ok_or(MobileSessionError::NotFound)?;
    write_store(&store).await?;
    Ok(UpsertMobileSessionResponse { ok: true, session })
}

pub async fn delete_session(owner: &str, id: &str) -> Result<DeleteMobileSessionResponse> {
    validate_id(id)?;
    let _guard = STORE_LOCK.lock().await;
    let mut store = read_store().await?;
    migrate_legacy_entries(&mut store, owner);
    store.remove(&store_key(owner, id));
    write_store(&store).await?;
    Ok(DeleteMobileSessionResponse { ok: true })
}

fn summary(session: &MobileSession) -> MobileSessionSummary {
    MobileSessionSummary {
        id: session.id.clone(),
        title: session.title.clone(),
        first_message_preview: session.first_message_preview.clone(),
        turn_count: session.turn_count,
        injected_op_count: session.injected_op_count,
        created_at: session.created_at,
        updated_at: session.updated_at,
        pinned_at: session.pinned_at,
    }
}

fn sort_summaries(sessions: &mut [MobileSessionSummary]) {
    sessions.sort_by(|a, b| {
        let a_pin = a.pinned_at.unwrap_or(0);
        let b_pin = b.pinned_at.unwrap_or(0);
        b_pin
            .cmp(&a_pin)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.title.cmp(&b.title))
    });
}

fn upsert_into_store(
    store: &mut BTreeMap<String, MobileSession>,
    owner: &str,
    path_id: &str,
    mut session: MobileSession,
) -> Result<()> {
    validate_session(&session)?;
    let key = store_key(owner, path_id);
    match store.get(&key) {
        Some(existing) => {
            // Additive optimistic-concurrency check on top of the existing
            // `updated_at` ordering guard: a caller must echo back the
            // `sync_version` it last observed. Either guard failing is a
            // stale update -- the client based its edit on out-of-date state.
            if existing.updated_at > session.updated_at
                || existing.sync_version != session.sync_version
            {
                return Err(MobileSessionError::StaleUpdate);
            }
            session.sync_version = existing.sync_version.saturating_add(1);
        }
        None => {
            // First write for this id: the client cannot know the server's
            // version yet, so any submitted value is accepted and the
            // session starts at version 1.
            session.sync_version = 1;
        }
    }
    store.insert(key, session);
    Ok(())
}

fn validate_session(session: &MobileSession) -> Result<()> {
    if session.title.len() > 256 {
        return Err(MobileSessionError::InvalidSession(
            "title must be at most 256 bytes".to_string(),
        ));
    }
    if session.first_message_preview.len() > 512 {
        return Err(MobileSessionError::InvalidSession(
            "first message preview must be at most 512 bytes".to_string(),
        ));
    }
    if session.created_at < 0 || session.updated_at < 0 {
        return Err(MobileSessionError::InvalidSession(
            "timestamps must be non-negative".to_string(),
        ));
    }
    if session.updated_at < session.created_at {
        return Err(MobileSessionError::InvalidSession(
            "updated_at must be greater than or equal to created_at".to_string(),
        ));
    }
    if session.pinned_at.is_some_and(|timestamp| timestamp < 0) {
        return Err(MobileSessionError::InvalidSession(
            "pinned_at must be non-negative".to_string(),
        ));
    }
    validate_sync_fields(session)?;

    let turn_count = usize::try_from(session.turn_count)
        .map_err(|_| MobileSessionError::InvalidSession("turn_count is too large".to_string()))?;
    let injected_op_count = usize::try_from(session.injected_op_count).map_err(|_| {
        MobileSessionError::InvalidSession("injected_op_count is too large".to_string())
    })?;
    let actual_turn_count = session
        .items
        .iter()
        .filter(|item| item.kind == "user")
        .count();
    let actual_injected_op_count = session
        .items
        .iter()
        .filter(|item| item.kind == "injection" || item.kind == "action_result")
        .count();
    if turn_count != actual_turn_count {
        return Err(MobileSessionError::InvalidSession(format!(
            "turn_count {turn_count} does not match {actual_turn_count} user items"
        )));
    }
    if injected_op_count != actual_injected_op_count {
        return Err(MobileSessionError::InvalidSession(format!(
            "injected_op_count {injected_op_count} does not match {actual_injected_op_count} operation items"
        )));
    }
    Ok(())
}

/// Bounds-checks the optimistic-concurrency/sync fields added for the Android
/// contract's Mobile Session Model (`sync_version`, `source_refs`, `draft`).
fn validate_sync_fields(session: &MobileSession) -> Result<()> {
    if session.sync_version < 0 {
        return Err(MobileSessionError::InvalidSession(
            "sync_version must be non-negative".to_string(),
        ));
    }
    if session.source_refs.len() > 256 {
        return Err(MobileSessionError::InvalidSession(
            "source_refs must have at most 256 entries".to_string(),
        ));
    }
    if session
        .source_refs
        .iter()
        .any(|reference| reference.len() > 512)
    {
        return Err(MobileSessionError::InvalidSession(
            "each source_refs entry must be at most 512 bytes".to_string(),
        ));
    }
    if session
        .draft
        .as_ref()
        .is_some_and(|draft| draft.len() > 8192)
    {
        return Err(MobileSessionError::InvalidSession(
            "draft must be at most 8192 bytes".to_string(),
        ));
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(MobileSessionError::InvalidId);
    }
    Ok(())
}

fn store_key(owner: &str, id: &str) -> String {
    format!("{owner}{STORE_KEY_SEPARATOR}{id}")
}

fn store_key_owner_matches(key: &str, owner: &str) -> bool {
    key.strip_prefix(owner)
        .is_some_and(|rest| rest.starts_with(STORE_KEY_SEPARATOR))
}

fn migrate_legacy_entries(store: &mut BTreeMap<String, MobileSession>, owner: &str) -> bool {
    let legacy_keys: Vec<String> = store
        .iter()
        .filter(|(key, session)| {
            !key.contains(STORE_KEY_SEPARATOR) && key.as_str() == session.id.as_str()
        })
        .map(|(key, _session)| key.clone())
        .collect();
    let mut migrated = false;
    for key in legacy_keys {
        let Some(session) = store.remove(&key) else {
            continue;
        };
        store
            .entry(store_key(owner, &session.id))
            .or_insert(session);
        migrated = true;
    }
    migrated
}

async fn read_store() -> Result<BTreeMap<String, MobileSession>> {
    let path = store_path();
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            if bytes.is_empty() {
                Ok(BTreeMap::new())
            } else {
                Ok(serde_json::from_slice(&bytes)?)
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(err) => Err(err.into()),
    }
}

async fn write_store(store: &BTreeMap<String, MobileSession>) -> Result<()> {
    let dir = store_dir();
    ensure_private_dir_async(dir.clone()).await?;
    let path = dir.join("sessions.json");
    let tmp = dir.join("sessions.json.tmp");
    let bytes = serde_json::to_vec_pretty(store)?;
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(tmp, path).await?;
    Ok(())
}

fn store_dir() -> PathBuf {
    axon_data_base_dir().join("mobile-sessions")
}

fn store_path() -> PathBuf {
    store_dir().join("sessions.json")
}

#[cfg(test)]
#[path = "mobile_sessions_tests.rs"]
mod tests;
