use axon_core::paths::axon_home_dir;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand_core::{OsRng, TryRngCore as _};
use std::io::{self, ErrorKind};
use std::path::PathBuf;
use subtle::ConstantTimeEq;

const PANEL_PASSWORD_FILE: &str = "panel-password";

#[derive(Clone)]
pub(crate) struct PanelPassword {
    token: String,
}

pub(crate) struct PanelPasswordInit {
    pub password: PanelPassword,
    pub generated: bool,
}

impl PanelPassword {
    pub fn verify(&self, candidate: &str) -> bool {
        self.token
            .as_bytes()
            .ct_eq(candidate.trim().as_bytes())
            .into()
    }

    pub fn as_str(&self) -> &str {
        &self.token
    }
}

pub(crate) fn init_panel_password() -> io::Result<PanelPasswordInit> {
    let home = axon_home_dir().ok_or_else(|| {
        io::Error::new(
            ErrorKind::NotFound,
            "HOME is unset or invalid; cannot initialize ~/.axon panel password",
        )
    })?;
    axon_services::setup::config_store::ensure_private_dir(&home)?;
    let path = home.join(PANEL_PASSWORD_FILE);

    if let Some(token) = read_existing_password(&path)? {
        return Ok(PanelPasswordInit {
            password: PanelPassword { token },
            generated: false,
        });
    }

    let token = generate_password()?;
    match create_password_file(&path, &token) {
        Ok(()) => Ok(PanelPasswordInit {
            password: PanelPassword { token },
            generated: true,
        }),
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            let token = read_existing_password(&path)?.ok_or_else(|| {
                io::Error::new(
                    ErrorKind::NotFound,
                    "panel password file appeared but could not be read",
                )
            })?;
            Ok(PanelPasswordInit {
                password: PanelPassword { token },
                generated: false,
            })
        }
        Err(err) => Err(err),
    }
}

fn generate_password() -> io::Result<String> {
    let mut bytes = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|e| io::Error::other(format!("OsRng failed: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn read_existing_password(path: &PathBuf) -> io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(value) => {
            let token = value.trim().to_string();
            if token.is_empty() {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!("panel password file '{}' is empty", path.display()),
                ));
            }
            Ok(Some(token))
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn create_password_file(path: &PathBuf, token: &str) -> io::Result<()> {
    use std::io::Write as _;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);

    let mut file = options.open(path)?;
    file.write_all(token.as_bytes())?;
    file.write_all(b"\n")?;
    #[cfg(unix)]
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    Ok(())
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
