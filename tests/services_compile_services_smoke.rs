#[test]
fn services_module_exports_exist() {
    let _ = axon::services::events::ServiceEvent::Log {
        level: axon::services::events::LogLevel::Info,
        message: "ok".to_string(),
    };
}
