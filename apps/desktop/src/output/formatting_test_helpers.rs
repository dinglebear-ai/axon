#[cfg(test)]
pub(in crate::output) fn actionable_error_text(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if let Some(index) = lines
        .iter()
        .position(|line| line.trim_start().starts_with("Error:"))
    {
        return lines[index..].join("\n");
    }

    let non_log_lines: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.contains(" WARN ")
                || trimmed.contains(" INFO ")
                || trimmed.contains(" DEBUG ")
                || trimmed.contains(" TRACE "))
        })
        .collect();

    if non_log_lines.is_empty() {
        text.to_string()
    } else {
        non_log_lines.join("\n")
    }
}

/// Strip ANSI / VT escape sequences. CSI, OSC, DCS, APC, PM, and SOS are
/// covered; malformed sequences are silently dropped.
#[cfg(test)]
pub(in crate::output) fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        let Some(&next) = chars.peek() else {
            continue;
        };
        match next {
            '[' => {
                chars.next();
                for ch in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&ch) {
                        break;
                    }
                }
            }
            ']' => {
                chars.next();
                consume_until_string_terminator(&mut chars, true);
            }
            'P' | '_' | '^' | 'X' => {
                chars.next();
                consume_until_string_terminator(&mut chars, false);
            }
            _ => {
                chars.next();
            }
        }
    }
    out
}

#[cfg(test)]
fn consume_until_string_terminator(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    allow_bel: bool,
) {
    while let Some(ch) = chars.next() {
        if allow_bel && ch == '\x07' {
            return;
        }
        if ch == '\x1b' {
            if chars.peek() == Some(&'\\') {
                chars.next();
                return;
            }
            continue;
        }
    }
}

#[cfg(test)]
pub(in crate::output) fn format_exit_status(status: &std::process::ExitStatus) -> String {
    if status.success() {
        return "ok".to_string();
    }
    if let Some(code) = status.code() {
        return format!("exit {code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            let name = signal_name(sig).unwrap_or("signal");
            return format!("killed by {name} ({sig})");
        }
    }
    format!("{status}")
}

#[cfg(unix)]
#[cfg(test)]
fn signal_name(sig: i32) -> Option<&'static str> {
    match sig {
        1 => Some("SIGHUP"),
        2 => Some("SIGINT"),
        3 => Some("SIGQUIT"),
        6 => Some("SIGABRT"),
        9 => Some("SIGKILL"),
        11 => Some("SIGSEGV"),
        13 => Some("SIGPIPE"),
        14 => Some("SIGALRM"),
        15 => Some("SIGTERM"),
        _ => None,
    }
}
