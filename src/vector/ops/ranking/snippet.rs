use std::collections::HashSet;

use super::AskCandidate;

/// Strip markdown inline formatting from a single line of text.
/// - Images `![alt](url)` are removed entirely.
/// - Links `[text](url)` are replaced with just the text.
///
/// Uses `char_indices` and a peekable iterator to avoid allocating a `Vec<char>`.
fn strip_markdown_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut iter = text.char_indices().peekable();

    while let Some((byte_pos, ch)) = iter.next() {
        // Image: ![...](url) — drop entirely
        if ch == '!' {
            if iter.peek().map(|(_, c)| *c) == Some('[') {
                iter.next(); // consume '['
                // Skip alt text until ']'
                for (_, c) in iter.by_ref() {
                    if c == ']' {
                        break;
                    }
                }
                // Consume '(url)' if present
                if iter.peek().map(|(_, c)| *c) == Some('(') {
                    iter.next(); // consume '('
                    for (_, c) in iter.by_ref() {
                        if c == ')' {
                            break;
                        }
                    }
                }
                continue;
            }
            out.push(ch);
            continue;
        }

        // Link: [text](url) — keep text only
        if ch == '[' {
            // byte_pos is the position of '['; text starts one byte after
            let text_start_byte = byte_pos + ch.len_utf8();
            let mut text_end_byte = text_start_byte;
            let mut found_close_bracket = false;

            // Scan for ']', tracking byte positions via char_indices
            loop {
                match iter.peek().copied() {
                    None => break,
                    Some((pos, ']')) => {
                        text_end_byte = pos;
                        found_close_bracket = true;
                        iter.next(); // consume ']'
                        break;
                    }
                    Some(_) => {
                        let Some((_, consumed)) = iter.next() else {
                            break;
                        };
                        text_end_byte += consumed.len_utf8();
                    }
                }
            }

            if !found_close_bracket {
                // Ran off end without ']' — emit literal '[' plus text consumed so far
                out.push('[');
                out.push_str(&text[text_start_byte..text_end_byte]);
                continue;
            }

            // Check for '(' following ']'
            if iter.peek().map(|(_, c)| *c) == Some('(') {
                iter.next(); // consume '('
                for (_, c) in iter.by_ref() {
                    if c == ')' {
                        break;
                    }
                }
                // Emit only the link text (byte-slice from original — valid since
                // text_start_byte and text_end_byte come from char_indices positions)
                out.push_str(&text[text_start_byte..text_end_byte]);
            } else {
                // Not a link — emit literal '[' + text + ']' (']' was consumed above)
                out.push('[');
                out.push_str(&text[text_start_byte..text_end_byte]);
                out.push(']');
            }
            continue;
        }

        out.push(ch);
    }
    out
}

fn markdown_link_count(text: &str) -> usize {
    text.match_indices("](").count()
}

fn looks_like_page_title(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.len() > 120 {
        return false;
    }
    if trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?') {
        return false;
    }
    if !(trimmed.contains(" - ") || trimmed.contains(" | ") || trimmed.contains(" :: ")) {
        return false;
    }
    let word_count = trimmed.split_whitespace().count();
    (3..=14).contains(&word_count)
}

fn is_boilerplate_line(raw_line: &str, cleaned_line: &str) -> bool {
    const NAV: &[&str] = &[
        "prev",
        "next",
        "home",
        "menu",
        "read more",
        "copy",
        "subscribe",
        "search",
        "search...",
        "navigation",
        "skip to main content",
        "model context protocol home page",
        "was this page helpful",
        "table of contents",
        "on this page",
        "developer docs",
    ];

    let lower = cleaned_line.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return true;
    }
    if NAV.iter().any(|p| lower == *p) {
        return true;
    }
    if markdown_link_count(raw_line) >= 3 {
        return true;
    }
    looks_like_page_title(cleaned_line)
}

/// Check if a line is a markdown horizontal rule (---, ***, ___) without
/// allocating a filtered String. Counts non-whitespace chars inline.
fn is_horizontal_rule(line: &str) -> bool {
    let mut count = 0usize;
    let mut rule_char: Option<char> = None;
    for c in line.chars() {
        if c.is_whitespace() {
            continue;
        }
        match rule_char {
            None => {
                if c == '-' || c == '*' || c == '_' {
                    rule_char = Some(c);
                    count += 1;
                } else {
                    return false;
                }
            }
            Some(rc) => {
                if c == rc {
                    count += 1;
                } else {
                    return false;
                }
            }
        }
    }
    count >= 3
}

/// Clean chunk text for snippet display: strip markdown structure, navigation
/// boilerplate, and bare URLs so sentence extraction sees prose only.
fn clean_snippet_source(text: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for line in text.lines() {
        let raw_line = line.trim();
        if raw_line.is_empty() || raw_line.starts_with('#') {
            continue;
        }
        // Horizontal rules — check inline without allocating a filtered String (MED-2).
        if is_horizontal_rule(raw_line) {
            continue;
        }
        // Strip leading list markers
        let line = raw_line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .or_else(|| line.strip_prefix("• "))
            .unwrap_or(raw_line);
        // Strip inline markdown then bare URLs
        let stripped = strip_markdown_inline(line);
        let filtered: String = stripped
            .split_whitespace()
            .filter(|w| !w.starts_with("http://") && !w.starts_with("https://"))
            .collect::<Vec<_>>()
            .join(" ");
        if !is_boilerplate_line(raw_line, &filtered) {
            parts.push(filtered);
        }
    }
    parts.join(" ")
}

/// True if a sentence is worth showing: has enough length, word count, and
/// contains real prose (not a bare URL or symbol line).
/// Requires ≥5 words, matching the TS reference (`split.length < 5` → reject).
fn is_relevant_sentence(s: &str) -> bool {
    let t = s.trim();
    t.len() >= 25
        && t.split_whitespace().count() >= 5
        && t.chars().any(|c| c.is_alphabetic())
        && !t.starts_with("http://")
        && !t.starts_with("https://")
        && t.chars().any(|c| c.is_alphanumeric())
}

/// Score one sentence against query tokens. +2 per matching token; +4 bonus
/// if the full query phrase appears verbatim (≥6 chars, ≥2 tokens).
fn score_sentence(sentence: &str, tokens: &[String], phrase: &str) -> usize {
    let lower = sentence.to_ascii_lowercase();
    let mut score: usize = tokens.iter().filter(|t| lower.contains(t.as_str())).count() * 2;
    if phrase.len() >= 6 && tokens.len() >= 2 && lower.contains(phrase) {
        score += 4;
    }
    score
}

/// Score a chunk for display preview quality: query relevance × sentence richness.
///
/// Formula (ported from TS `scoreChunkForPreview`):
///   `relevance_score * 10 + richness`
/// where richness = `min(sentence_count, 5) * 2 + min(char_count, 500) / 100`.
///
/// This is intentionally independent of the vector score so a dense code block
/// doesn't win the preview slot over a chunk with rich explanatory prose.
fn score_chunk_for_preview(text: &str, query_tokens: &[String]) -> f32 {
    let phrase = query_tokens.join(" ");
    let cleaned = clean_snippet_source(text);
    let sentences: Vec<&str> = cleaned
        .split(['.', '!', '?'])
        .map(str::trim)
        .filter(|s| is_relevant_sentence(s))
        .collect();
    let relevance: usize = sentences
        .iter()
        .map(|s| score_sentence(s, query_tokens, &phrase))
        .sum();
    let richness = sentences.len().min(5) as f32 * 2.0 + (cleaned.len().min(500) as f32) / 100.0;
    relevance as f32 * 10.0 + richness
}

/// From all chunks belonging to `url` in `candidates`, return the index of the
/// chunk with the highest preview score (prose richness × query relevance).
/// Scans up to 8 candidates per URL. Falls back to `fallback_idx` if none match.
///
/// Ported from TS `selectBestPreviewItem` in `utils/snippet.ts`.
pub fn select_best_preview_chunk(
    candidates: &[AskCandidate],
    url: &str,
    query_tokens: &[String],
    fallback_idx: usize,
) -> usize {
    candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| c.url == url)
        .take(8)
        .map(|(i, c)| (i, score_chunk_for_preview(&c.chunk_text, query_tokens)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(fallback_idx)
}

/// Extract a compact, query-relevant snippet from chunk text.
/// Ported from axon/src/utils/snippet.ts `getMeaningfulSnippet`.
/// Steps: clean → split sentences → filter nav → score per token → select ≤5
/// (≤700 chars) in doc order → pad if <3 → fallback to first 5 / 220 chars.
pub fn get_meaningful_snippet(text: &str, query_tokens: &[String]) -> String {
    let phrase = query_tokens.join(" ");
    let cleaned = clean_snippet_source(text);

    let sentences: Vec<&str> = cleaned
        .split(['.', '!', '?'])
        .map(str::trim)
        .filter(|s| is_relevant_sentence(s))
        .collect();

    if sentences.is_empty() {
        // TS fallback: find the first prose line (≥20 chars, alphabetic), then truncate.
        let fallback = cleaned
            .split('\n')
            .map(str::trim)
            .find(|l| l.len() >= 20 && l.chars().any(|c| c.is_alphabetic()))
            .unwrap_or(&cleaned);
        let end = fallback
            .char_indices()
            .nth(220)
            .map(|(i, _)| i)
            .unwrap_or(fallback.len());
        return fallback[..end].to_string();
    }

    if !query_tokens.is_empty() {
        let mut scored: Vec<(usize, usize)> = sentences
            .iter()
            .enumerate()
            .map(|(i, s)| (i, score_sentence(s, query_tokens, &phrase)))
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        let mut selected: Vec<usize> = Vec::new();
        let mut total_len = 0usize;
        for &(idx, score) in &scored {
            if score == 0 {
                break;
            }
            if selected.len() >= 5 {
                break;
            }
            if total_len + sentences[idx].len() > 700 && selected.len() >= 3 {
                break;
            }
            selected.push(idx);
            total_len += sentences[idx].len();
        }

        // Pad with contextually adjacent sentences if we have fewer than 3.
        if !selected.is_empty() && selected.len() < 3 {
            let anchor = *selected.iter().min().unwrap_or(&0);
            let selected_set: HashSet<usize> = selected.iter().copied().collect();
            let mut pads: Vec<usize> = (0..sentences.len())
                .filter(|i| !selected_set.contains(i))
                .collect();
            pads.sort_by_key(|&i| (i as isize - anchor as isize).unsigned_abs());
            for idx in pads {
                if selected.len() >= 3 {
                    break;
                }
                if total_len + sentences[idx].len() > 700 && selected.len() >= 2 {
                    break;
                }
                total_len += sentences[idx].len();
                selected.push(idx);
            }
        }

        if !selected.is_empty() {
            selected.sort_unstable();
            return selected
                .iter()
                .map(|&i| sentences[i])
                .collect::<Vec<_>>()
                .join(" ");
        }
    }

    // No query or no term matches: take first 3-5 sentences.
    let mut out: Vec<&str> = Vec::new();
    let mut total_len = 0usize;
    for &s in &sentences {
        if out.len() >= 5 {
            break;
        }
        if total_len + s.len() > 700 && out.len() >= 3 {
            break;
        }
        out.push(s);
        total_len += s.len();
    }
    out.join(" ")
}
