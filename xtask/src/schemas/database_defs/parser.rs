//! Low-level statement parsing helpers for `database_defs`. Split out of the
//! module root to stay under the repo's 500-line-per-file monolith cap; see
//! `xtask/src/schemas/database_defs.rs` for the orchestration and data
//! model that consumes these helpers.

#[derive(Debug, Clone, Default)]
pub(super) struct ForeignKey {
    pub columns: Vec<String>,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    pub on_delete: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Column {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub primary_key: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Table {
    pub name: String,
    pub owner_crate: &'static str,
    pub introduced_in: String,
    pub columns: Vec<Column>,
    pub primary_key: Vec<String>,
    pub foreign_keys: Vec<ForeignKey>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct IndexDef {
    pub name: String,
    pub table: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub where_clause: Option<String>,
}

/// Split a migration file's text into individual statements on top-level
/// `;` (none of these files embed a semicolon inside a string literal or
/// nested parens, so this is a safe simplification for this dialect).
pub(super) fn split_statements(text: &str) -> Vec<String> {
    let mut stripped = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("--") {
            continue;
        }
        stripped.push_str(line);
        stripped.push('\n');
    }
    stripped
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Collapse a statement to single-spaced text for regex-free token scanning.
pub(super) fn normalize(stmt: &str) -> String {
    stmt.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Find the balanced `(...)` starting at or after `from`, returning the
/// inner content and the index just past the closing paren.
pub(super) fn balanced_parens(text: &str, from: usize) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    let open = text[from..].find('(')? + from;
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((text[open + 1..i].to_string(), i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Split a comma-separated list at top-level (paren-depth 0) commas.
pub(super) fn split_top_level(text: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in text.chars() {
        match ch {
            '(' => {
                depth += 1;
                cur.push(ch);
            }
            ')' => {
                depth -= 1;
                cur.push(ch);
            }
            c if c == sep && depth == 0 => {
                out.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

pub(super) fn clean_column_ref(raw: String) -> String {
    // Strip trailing sort direction/collation tokens like "col DESC".
    raw.split_whitespace()
        .next()
        .unwrap_or(&raw)
        .trim_matches(|c: char| c == '"' || c == '`')
        .to_string()
}

pub(super) fn parse_column_def(entry: &str) -> Column {
    let mut parts = entry.split_whitespace();
    let name = parts.next().unwrap_or_default().to_string();
    let sql_type = parts.next().unwrap_or_default().to_uppercase();
    let upper = entry.to_uppercase();
    Column {
        name,
        sql_type,
        nullable: !upper.contains("NOT NULL"),
        primary_key: upper.contains("PRIMARY KEY"),
    }
}

pub(super) fn extract_on_delete(rest: &str) -> Option<String> {
    let upper = rest.to_uppercase();
    let idx = upper.find("ON DELETE")?;
    let after = rest[idx + "ON DELETE".len()..].trim_start();
    let action: String = after
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take_while(|c| c.is_ascii_alphabetic() || *c == ' ')
        .collect();
    let action = action.trim();
    if action.is_empty() {
        None
    } else {
        Some(action.to_uppercase())
    }
}

pub(super) fn inline_column_fk(entry: &str, column: &str) -> Option<ForeignKey> {
    let upper = entry.to_uppercase();
    let idx = upper.find("REFERENCES")?;
    let after = &entry[idx + "REFERENCES".len()..];
    let after_trim = after.trim_start();
    let name_end = after_trim.find('(').unwrap_or(after_trim.len());
    let ref_table = after_trim[..name_end].trim().to_string();
    let paren_start = after_trim.find('(')?;
    let (cols, rest_start) = balanced_parens(after_trim, paren_start)?;
    let ref_columns = split_top_level(&cols, ',')
        .into_iter()
        .map(clean_column_ref)
        .collect();
    let on_delete = extract_on_delete(&after_trim[rest_start..]);
    Some(ForeignKey {
        columns: vec![column.to_string()],
        ref_table,
        ref_columns,
        on_delete,
    })
}

pub(super) fn parse_foreign_key_constraint(entry: &str) -> Option<ForeignKey> {
    let open = entry.find('(')?;
    let (cols, after_cols_idx) = balanced_parens(entry, open)?;
    let columns = split_top_level(&cols, ',')
        .into_iter()
        .map(clean_column_ref)
        .collect();
    let rest = &entry[after_cols_idx..];
    let upper = rest.to_uppercase();
    let ref_idx = upper.find("REFERENCES")?;
    let after = &rest[ref_idx + "REFERENCES".len()..].trim_start();
    let name_end = after.find('(').unwrap_or(after.len());
    let ref_table = after[..name_end].trim().to_string();
    let paren_start = after.find('(')?;
    let (ref_cols, rest_start) = balanced_parens(after, paren_start)?;
    let ref_columns = split_top_level(&ref_cols, ',')
        .into_iter()
        .map(clean_column_ref)
        .collect();
    let on_delete = extract_on_delete(&after[rest_start..]);
    Some(ForeignKey {
        columns,
        ref_table,
        ref_columns,
        on_delete,
    })
}

pub(super) fn parse_create_table(
    stmt: &str,
    owner_crate: &'static str,
    file: &str,
) -> Option<Table> {
    let after_kw = stmt
        .strip_prefix("CREATE TABLE ")
        .or_else(|| stmt.strip_prefix("CREATE TABLE"))?;
    let after_kw = after_kw
        .trim_start()
        .strip_prefix("IF NOT EXISTS ")
        .unwrap_or(after_kw.trim_start());
    let name_end = after_kw.find(['(', ' ']).unwrap_or(after_kw.len());
    let name = clean_column_ref(after_kw[..name_end].trim().to_string());
    let paren_start = after_kw.find('(')?;
    let (body, _) = balanced_parens(after_kw, paren_start)?;

    let mut table = Table {
        name,
        owner_crate,
        introduced_in: file.to_string(),
        ..Default::default()
    };

    for entry in split_top_level(&body, ',') {
        let entry_upper = entry.to_uppercase();
        if entry_upper.starts_with("PRIMARY KEY") {
            if let Some(open) = entry.find('(') {
                if let Some((cols, _)) = balanced_parens(&entry, open) {
                    table.primary_key = split_top_level(&cols, ',')
                        .into_iter()
                        .map(clean_column_ref)
                        .collect();
                }
            }
        } else if entry_upper.starts_with("FOREIGN KEY") {
            if let Some(fk) = parse_foreign_key_constraint(&entry) {
                table.foreign_keys.push(fk);
            }
        } else if entry_upper.starts_with("UNIQUE") || entry_upper.starts_with("CHECK") {
            // Table-level UNIQUE/CHECK constraints don't change table shape
            // in a way this artifact tracks beyond indexes; intentionally
            // skipped here (composite UNIQUE constraints created via
            // `CREATE UNIQUE INDEX` are captured separately).
            continue;
        } else {
            let column = parse_column_def(&entry);
            if column.primary_key {
                table.primary_key.push(column.name.clone());
            }
            if let Some(fk) = inline_column_fk(&entry, &column.name) {
                table.foreign_keys.push(fk);
            }
            table.columns.push(column);
        }
    }

    Some(table)
}

pub(super) fn parse_create_index(stmt: &str) -> Option<IndexDef> {
    let unique = stmt.to_uppercase().starts_with("CREATE UNIQUE INDEX");
    let after_kw = if unique {
        stmt.strip_prefix("CREATE UNIQUE INDEX ")?
    } else {
        stmt.strip_prefix("CREATE INDEX ")?
    };
    let after_kw = after_kw.strip_prefix("IF NOT EXISTS ").unwrap_or(after_kw);
    let on_idx = after_kw.find(" ON ")?;
    let name = after_kw[..on_idx].trim().to_string();
    let after_on = &after_kw[on_idx + " ON ".len()..];
    let paren_start = after_on.find('(')?;
    let table = clean_column_ref(after_on[..paren_start].trim().to_string());
    let (cols_raw, rest_start) = balanced_parens(after_on, paren_start)?;
    let columns = split_top_level(&cols_raw, ',');
    let rest = after_on[rest_start..].trim();
    let where_clause = rest
        .to_uppercase()
        .starts_with("WHERE")
        .then(|| rest["WHERE".len()..].trim().to_string());
    Some(IndexDef {
        name,
        table,
        columns,
        unique,
        where_clause,
    })
}

pub(super) fn parse_drop_table(stmt: &str) -> Option<String> {
    let after = stmt
        .strip_prefix("DROP TABLE ")
        .or_else(|| stmt.strip_prefix("DROP TABLE"))?;
    let after = after
        .trim_start()
        .strip_prefix("IF EXISTS ")
        .unwrap_or(after.trim_start());
    Some(after.trim().trim_end_matches(';').to_string())
}
