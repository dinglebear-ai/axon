use crate::{BackendProduct, persistence::atomic_write};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};

const FILE: &str = "backend-credentials.json";

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredBackendCredential {
    pub handle: String,
    pub profile_id: String,
    pub product: BackendProduct,
    pub origin: String,
    pub server_id: String,
    pub generation: String,
    #[serde(skip_serializing)]
    pub token: String,
}

#[derive(Deserialize, Serialize)]
struct VaultEntry {
    profile_id: String,
    product: BackendProduct,
    origin: String,
    server_id: String,
    generation: String,
    token: String,
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|p| p.join(FILE))
        .map_err(|e| e.to_string())
}
fn read(path: &Path) -> Result<HashMap<String, VaultEntry>, String> {
    match std::fs::read(path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|_| "credential vault is corrupt".into())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(e.to_string()),
    }
}
pub(crate) fn load(
    app: &AppHandle,
    handle: &str,
) -> Result<Option<StoredBackendCredential>, String> {
    let Some(entry) = read(&path(app)?)?.remove(handle) else {
        return Ok(None);
    };
    Ok(Some(StoredBackendCredential {
        handle: handle.into(),
        profile_id: entry.profile_id,
        product: entry.product,
        origin: entry.origin,
        server_id: entry.server_id,
        generation: entry.generation,
        token: entry.token,
    }))
}
#[tauri::command]
pub(crate) fn save_backend_credential(
    app: AppHandle,
    credential: StoredBackendCredential,
) -> Result<(), String> {
    if credential.handle.is_empty()
        || credential.handle.len() > 128
        || credential.profile_id.len() > 128
        || credential.token.is_empty()
        || credential.token.len() > 16 * 1024
        || credential.server_id.is_empty()
        || credential.generation.is_empty()
    {
        return Err("invalid backend credential binding".into());
    }
    let vault_path = path(&app)?;
    let mut entries = read(&vault_path)?;
    entries.retain(|handle, entry| {
        handle == &credential.handle || entry.profile_id != credential.profile_id
    });
    entries.insert(
        credential.handle,
        VaultEntry {
            profile_id: credential.profile_id,
            product: credential.product,
            origin: credential.origin,
            server_id: credential.server_id,
            generation: credential.generation,
            token: credential.token,
        },
    );
    atomic_write(
        &vault_path,
        &serde_json::to_vec(&entries).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
#[tauri::command]
pub(crate) fn delete_backend_credential(app: AppHandle, profile_id: String) -> Result<(), String> {
    let vault_path = path(&app)?;
    let mut entries = read(&vault_path)?;
    entries.retain(|_, entry| entry.profile_id != profile_id);
    atomic_write(
        &vault_path,
        &serde_json::to_vec(&entries).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
