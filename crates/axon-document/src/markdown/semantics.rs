//! Linear semantic-boundary discovery for Markdown prose windows.

#[derive(Clone, Copy)]
pub(super) struct CharBlock {
    pub(super) start: usize,
    pub(super) end: usize,
}

pub(super) struct SemanticLayout {
    pub(super) line_boundaries: Vec<usize>,
    paragraph_boundaries: Vec<usize>,
    structural_boundaries: Vec<usize>,
    pub(super) structural_blocks: Vec<CharBlock>,
}

impl SemanticLayout {
    pub(super) fn new(content: &str, max_chars: usize) -> Self {
        let mut lines = Vec::new();
        let mut start = 0usize;
        for text in content.split_inclusive('\n') {
            let end = start + text.chars().count();
            let content_end = start + text.trim_end_matches(['\r', '\n']).chars().count();
            lines.push(SemanticLine {
                start,
                end,
                content_end,
                text,
            });
            start = end;
        }

        let mut layout = Self {
            line_boundaries: lines.iter().map(|line| line.end).collect(),
            paragraph_boundaries: lines
                .iter()
                .filter(|line| line.text.trim().is_empty())
                .map(|line| line.end)
                .collect(),
            structural_boundaries: Vec::new(),
            structural_blocks: Vec::new(),
        };
        layout.collect_structural_blocks(&lines, max_chars);
        layout
    }

    fn collect_structural_blocks(&mut self, lines: &[SemanticLine<'_>], max_chars: usize) {
        let mut index = 0usize;
        while index < lines.len() {
            let is_table_start = index + 1 < lines.len()
                && lines[index].text.contains('|')
                && is_table_delimiter(lines[index + 1].text);
            let is_list_start = is_list_item(lines[index].text);
            if !is_table_start && !is_list_start {
                index += 1;
                continue;
            }

            let start = lines[index].start;
            let mut end_index = index + usize::from(is_table_start) + 1;
            if is_table_start {
                while end_index < lines.len()
                    && !lines[end_index].text.trim().is_empty()
                    && lines[end_index].text.contains('|')
                {
                    end_index += 1;
                }
            } else {
                while end_index < lines.len() && is_list_item(lines[end_index].text) {
                    end_index += 1;
                }
            }
            self.push_structural_block(lines, start, end_index, max_chars);
            index = end_index;
        }
    }

    fn push_structural_block(
        &mut self,
        lines: &[SemanticLine<'_>],
        start: usize,
        end_index: usize,
        max_chars: usize,
    ) {
        let last = &lines[end_index - 1];
        self.structural_boundaries.extend([start, last.end]);
        if last.content_end.saturating_sub(start) <= max_chars {
            self.structural_blocks.push(CharBlock {
                start,
                end: last.end,
            });
        }
    }
}

struct SemanticLine<'a> {
    start: usize,
    end: usize,
    content_end: usize,
    text: &'a str,
}

pub(super) fn preferred_boundary(
    layout: &SemanticLayout,
    target: usize,
    floor: usize,
    paragraph_cursor: &mut usize,
    structural_cursor: &mut usize,
    line_cursor: &mut usize,
) -> Option<usize> {
    latest_boundary(
        &layout.paragraph_boundaries,
        paragraph_cursor,
        target,
        floor,
    )
    .or_else(|| {
        latest_boundary(
            &layout.structural_boundaries,
            structural_cursor,
            target,
            floor,
        )
    })
    .or_else(|| latest_boundary(&layout.line_boundaries, line_cursor, target, floor))
}

pub(super) fn latest_boundary(
    boundaries: &[usize],
    cursor: &mut usize,
    target: usize,
    floor: usize,
) -> Option<usize> {
    while *cursor < boundaries.len() && boundaries[*cursor] <= target {
        *cursor += 1;
    }
    cursor
        .checked_sub(1)
        .map(|index| boundaries[index])
        .filter(|boundary| *boundary > floor)
}

pub(super) fn containing_block(
    blocks: &[CharBlock],
    cursor: &mut usize,
    position: usize,
) -> Option<CharBlock> {
    while *cursor < blocks.len() && blocks[*cursor].end <= position {
        *cursor += 1;
    }
    blocks
        .get(*cursor)
        .copied()
        .filter(|block| block.start <= position && position < block.end)
}

fn is_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    if ["- ", "* ", "+ "]
        .iter()
        .any(|marker| trimmed.starts_with(marker))
    {
        return true;
    }
    let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0
        && trimmed[digits..]
            .strip_prefix(['.', ')'])
            .is_some_and(|rest| rest.starts_with([' ', '\t']))
}

fn is_table_delimiter(line: &str) -> bool {
    let trimmed = line.trim().trim_matches('|');
    let mut cells = trimmed.split('|');
    let mut count = 0usize;
    cells.all(|cell| {
        count += 1;
        let rule = cell.trim().trim_matches(':');
        rule.len() >= 3 && rule.bytes().all(|byte| byte == b'-')
    }) && count > 0
}
