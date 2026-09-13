use super::*;

#[test]
fn generated_password_notice_identifies_protected_file_without_secret() {
    let notice = new_password_notice(
        std::path::Path::new("/private/panel-password"),
        "127.0.0.1",
        8001,
    );

    assert_eq!(
        notice,
        "Axon web panel password generated at /private/panel-password\nOpen: http://127.0.0.1:8001"
    );
}
