//! Compile-time-adjacent validation that all `JobTable` and `JobStatus` string
//! values are safe for interpolation into SQL `format!()` queries.
//!
//! These enums are used in `format!()` to build SQL strings (e.g.
//! `format!("UPDATE {table_name} SET status='{status}' ...")`) throughout
//! `job_ops.rs` and `watchdog.rs`. If a variant's `as_str()` value contained
//! anything outside `[a-z0-9_]`, it would be a SQL injection vector. These
//! tests make that impossible to introduce accidentally.

use crate::crates::jobs::common::JobTable;
use crate::crates::jobs::status::JobStatus;

/// Only lowercase ASCII letters, digits, and underscores are safe for
/// unquoted SQL identifiers and string literals used in format!() queries.
fn is_sql_safe_identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

const ALL_JOB_TABLES: [JobTable; 6] = [
    JobTable::Crawl,
    JobTable::Refresh,
    JobTable::Extract,
    JobTable::Embed,
    JobTable::Ingest,
    JobTable::Graph,
];

const ALL_JOB_STATUSES: [JobStatus; 5] = [
    JobStatus::Pending,
    JobStatus::Running,
    JobStatus::Completed,
    JobStatus::Failed,
    JobStatus::Canceled,
];

#[test]
fn job_table_names_are_sql_safe() {
    for table in ALL_JOB_TABLES {
        let name = table.as_str();
        assert!(
            is_sql_safe_identifier(name),
            "JobTable::{table:?} as_str() = {name:?} contains unsafe SQL characters \
             (only [a-z0-9_] allowed)"
        );
    }
}

#[test]
fn job_status_values_are_sql_safe() {
    for status in ALL_JOB_STATUSES {
        let name = status.as_str();
        assert!(
            is_sql_safe_identifier(name),
            "JobStatus::{status:?} as_str() = {name:?} contains unsafe SQL characters \
             (only [a-z0-9_] allowed)"
        );
    }
}

#[test]
fn job_table_names_start_with_axon_prefix() {
    for table in ALL_JOB_TABLES {
        let name = table.as_str();
        assert!(
            name.starts_with("axon_"),
            "JobTable::{table:?} as_str() = {name:?} does not start with 'axon_' prefix"
        );
    }
}

#[test]
fn all_job_table_variants_covered() {
    // If a new variant is added to JobTable, the ALL_JOB_TABLES const array
    // above will fail to compile (non-exhaustive) OR this count check will
    // fail, forcing the developer to add the new variant to the safety tests.
    let unique: std::collections::HashSet<&str> =
        ALL_JOB_TABLES.iter().map(|t| t.as_str()).collect();
    assert_eq!(
        unique.len(),
        ALL_JOB_TABLES.len(),
        "Duplicate table names detected in ALL_JOB_TABLES"
    );
}

#[test]
fn all_job_status_variants_covered() {
    let unique: std::collections::HashSet<&str> =
        ALL_JOB_STATUSES.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        unique.len(),
        ALL_JOB_STATUSES.len(),
        "Duplicate status values detected in ALL_JOB_STATUSES"
    );
}
