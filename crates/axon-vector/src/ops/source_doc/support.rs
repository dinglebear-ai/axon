use super::SourceOrigin;
use serde_json::{Map, Value};
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

pub(super) fn base_chunk_metadata(
    content_kind: &str,
    locator: &str,
    line_start: u32,
    line_end: u32,
    byte_start: usize,
    byte_end: usize,
) -> Map<String, Value> {
    let mut range = Map::new();
    range.insert("line_start".into(), line_start.into());
    range.insert("line_end".into(), line_end.into());
    range.insert("byte_start".into(), byte_start.into());
    range.insert("byte_end".into(), byte_end.into());

    let mut extra = Map::new();
    extra.insert("chunk_content_kind".into(), content_kind.into());
    extra.insert("chunk_locator".into(), locator.into());
    extra.insert("source_range".into(), Value::Object(range));
    extra
}

pub(super) fn chunk_metadata(metadata: Map<String, Value>) -> Value {
    Value::Object(metadata)
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

pub(super) fn insert_missing_or_null(map: &mut Map<String, Value>, key: &str, value: Value) {
    if !map.contains_key(key) || map.get(key).is_some_and(Value::is_null) {
        map.insert(key.to_string(), value);
    }
}
