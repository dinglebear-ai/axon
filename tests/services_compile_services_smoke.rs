#[test]
fn services_module_exports_exist() {
    let _ = axon_services::events::ServiceEvent::Log {
        level: axon_services::events::LogLevel::Info,
        message: "ok".to_string(),
    };
}
