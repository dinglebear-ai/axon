use super::*;

#[test]
fn core_dump_guard_is_noop_when_cache_disabled() {
    let cfg = Config {
        ask_cache_enabled: false,
        ..Config::default()
    };

    enforce_core_dump_disabled_for_ask_cache(&cfg).expect("disabled cache must not alter limits");
}
