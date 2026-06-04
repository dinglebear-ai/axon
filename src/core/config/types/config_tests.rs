use super::*;

#[test]
fn ask_hybrid_candidates_default_is_150() {
    let cfg = Config::default();
    assert_eq!(
        cfg.ask_hybrid_candidates, 150,
        "ask_hybrid_candidates should preserve a wider recall window for ask"
    );
}
