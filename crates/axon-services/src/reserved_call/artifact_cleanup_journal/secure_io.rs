use super::*;

mod names;
#[cfg(unix)]
use names::component_name;
use names::lease_name;
#[cfg(not(unix))]
mod replace;
#[cfg(not(unix))]
use replace::replace_file;
#[cfg(unix)]
mod temp;
#[cfg(unix)]
use temp::TempFile;

#[cfg(not(unix))]
mod non_unix;
#[cfg(not(unix))]
pub(super) use non_unix::SecureJournalDir;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(super) use unix::SecureJournalDir;
