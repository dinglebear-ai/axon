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
