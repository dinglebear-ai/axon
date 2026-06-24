use super::*;

#[test]
fn panel_password_uses_constant_time_verify() {
    let password = PanelPassword {
        token: "secret".to_string(),
    };

    assert!(password.verify("secret"));
    assert!(!password.verify("wrong"));
}
