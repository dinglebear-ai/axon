use super::*;

#[test]
fn strip_ansi_csi_colours() {
    let input = "\x1b[1;31merror\x1b[0m: thing";
    assert_eq!(strip_ansi(input), "error: thing");
}

#[test]
fn strip_ansi_csi_with_punctuation_final_byte() {
    // CSI final byte may be punctuation such as `~` or `@`
    let input = "before\x1b[2~after";
    assert_eq!(strip_ansi(input), "beforeafter");
}

#[test]
fn strip_ansi_osc_bel_terminated() {
    // OSC 0; set window title <BEL>
    let input = "pre\x1b]0;hello world\x07post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_osc_st_terminated() {
    // OSC terminated by ST = ESC '\\'
    let input = "pre\x1b]0;hello world\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_dcs_st_terminated() {
    // DCS = ESC 'P' … ST
    let input = "pre\x1bPq#0;2;0;0;0\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_apc_st_terminated() {
    // APC = ESC '_' … ST
    let input = "pre\x1b_some app cmd\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_pm_st_terminated() {
    // PM = ESC '^' … ST
    let input = "pre\x1b^private\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_sos_st_terminated() {
    // SOS = ESC 'X' … ST
    let input = "pre\x1bXstring\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_malformed_lone_esc() {
    // a lone trailing ESC is silently dropped
    let input = "tail\x1b";
    assert_eq!(strip_ansi(input), "tail");
}

#[test]
fn strip_ansi_malformed_unterminated_osc() {
    // OSC with no terminator before EOF — drop the rest
    let input = "pre\x1b]0;never ends";
    assert_eq!(strip_ansi(input), "pre");
}

#[test]
fn strip_ansi_plain_text_passthrough() {
    let input = "no escapes here\nmultiple\tlines";
    assert_eq!(strip_ansi(input), input);
}

#[test]
fn strip_ansi_multiple_mixed_sequences() {
    let input = "\x1b[31mred\x1b[0m \x1b]0;title\x07 \x1b_apc\x1b\\ done";
    assert_eq!(strip_ansi(input), "red   done");
}

#[test]
fn strip_ansi_short_two_byte_escape() {
    // ESC c (reset) — a two-byte Fp/Fs escape — should drop both bytes
    let input = "before\x1bcafter";
    assert_eq!(strip_ansi(input), "beforeafter");
}

#[test]
fn strip_ansi_handles_unicode_around_escapes() {
    let input = "café\x1b[1m → \x1b[0mok ✓";
    assert_eq!(strip_ansi(input), "café → ok ✓");
}

#[test]
fn strip_ansi_dcs_does_not_terminate_on_bel() {
    // Per ECMA-48, DCS terminates ONLY on ST (ESC \). An embedded BEL is
    // payload data, not a terminator, and must NOT short-circuit stripping.
    // If BEL were (incorrectly) treated as a DCS terminator, the trailing
    // "after\x1b\\post" would survive as "afterpost" — we'd see "preafterpost".
    let input = "pre\x1bPq#0;2;0;0;0\x07after\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_apc_does_not_terminate_on_bel() {
    // APC payload may contain BEL bytes; only ST terminates.
    let input = "pre\x1b_cmd\x07with\x07bels\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_pm_does_not_terminate_on_bel() {
    let input = "pre\x1b^private\x07message\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_sos_does_not_terminate_on_bel() {
    let input = "pre\x1bXstring\x07with\x07bels\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_osc_still_terminates_on_bel() {
    // Regression guard: OSC must KEEP its BEL-terminator behaviour
    // (xterm legacy convention) even though DCS/APC/PM/SOS reject BEL.
    let input = "pre\x1b]0;title\x07keep";
    assert_eq!(strip_ansi(input), "prekeep");
}
