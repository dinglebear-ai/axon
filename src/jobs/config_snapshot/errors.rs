pub(super) fn serde_json_error(message: String) -> serde_json::Error {
    serde_json::Error::io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    ))
}

pub(super) fn running_in_container() -> bool {
    crate::core::config::parse::docker::running_in_container()
}
