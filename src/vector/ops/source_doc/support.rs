use super::SourceOrigin;
use spider::url::Url;

pub(super) fn domain_from_web_url(url: &str) -> Result<String, String> {
    Url::parse(url)
        .map_err(|e| format!("invalid web URL {url}: {e}"))?
        .host_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("web URL missing host: {url}"))
}

pub(super) fn domain_for_origin(origin: SourceOrigin, url: &str) -> String {
    match origin {
        SourceOrigin::GitFile => Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "git".to_string()),
        SourceOrigin::LocalFile => "local".to_string(),
        _ => "unknown".to_string(),
    }
}

pub(super) fn file_locator(path: &str, line_start: u32, line_end: u32) -> String {
    if line_start == line_end {
        format!("{path}#L{line_start}")
    } else {
        format!("{path}#L{line_start}-L{line_end}")
    }
}

pub(super) fn locate_chunk(text: &str, chunk: &str, cursor: usize) -> (usize, usize) {
    let start = text[cursor..]
        .find(chunk)
        .map(|offset| cursor + offset)
        .or_else(|| text.find(chunk))
        .unwrap_or(cursor.min(text.len()));
    (start, start.saturating_add(chunk.len()).min(text.len()))
}

pub(super) struct LineIndex {
    text_len: usize,
    newline_offsets: Vec<usize>,
}

impl LineIndex {
    pub(super) fn new(text: &str) -> Self {
        Self {
            text_len: text.len(),
            newline_offsets: text
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index))
                .collect(),
        }
    }

    pub(super) fn line_range_for_bytes(&self, byte_start: usize, byte_end: usize) -> (u32, u32) {
        let line_start = self.line_for_byte(byte_start);
        let end = byte_end.saturating_sub(1);
        let line_end = self.line_for_byte(end.max(byte_start));
        (line_start, line_end.max(line_start))
    }

    fn line_for_byte(&self, byte: usize) -> u32 {
        let capped = byte.min(self.text_len);
        self.newline_offsets
            .partition_point(|offset| *offset < capped) as u32
            + 1
    }
}
