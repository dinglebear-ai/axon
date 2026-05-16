use super::*;

#[test]
fn disabled_record_is_noop() {
    let mut t = AskTiming::new(false, Instant::now());
    let probe = Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(2));
    t.record(AskTimingSlot::TeiEmbed, probe);
    t.set_ttft(42);
    t.set_streamed(true);
    assert!(t.enabled().is_none());
}

#[test]
fn enabled_record_populates() {
    let mut t = AskTiming::new(true, Instant::now());
    let probe = Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(2));
    t.record(AskTimingSlot::TeiEmbed, probe);
    t.set_ttft(99);
    t.set_streamed(false);
    let e = t.enabled().expect("enabled");
    assert!(e.tei_embed_ms.is_some_and(|v| v >= 1));
    assert_eq!(e.llm_ttft_ms, Some(99));
    assert_eq!(e.streamed, Some(false));
}

#[test]
fn disabled_helper_has_no_request_start() {
    let t = AskTiming::disabled();
    assert!(t.request_start().is_none());
    assert!(t.enabled().is_none());
}
