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
    // Verify each JobTable variant is present in ALL_JOB_TABLES.  The
    // exhaustive match closure below will fail to *compile* if a new variant
    // is added to the enum without being listed here, and the assert_contains
    // calls make the runtime test fail if the variant is in the match but was
    // accidentally omitted from the slice.
    let all: &[JobTable] = &ALL_JOB_TABLES;
    let assert_contains = |v: JobTable| {
        assert!(
            all.contains(&v),
            "ALL_JOB_TABLES is missing variant {v:?} — add it to the slice"
        );
    };
    // This closure is never called at runtime, but it *must* compile — the
    // exhaustive match forces a compile error when a new JobTable variant is
    // introduced without updating both this match and the ALL_JOB_TABLES slice.
    let _ = |t: JobTable| match t {
        JobTable::Crawl => assert_contains(JobTable::Crawl),
        JobTable::Refresh => assert_contains(JobTable::Refresh),
        JobTable::Extract => assert_contains(JobTable::Extract),
        JobTable::Embed => assert_contains(JobTable::Embed),
        JobTable::Ingest => assert_contains(JobTable::Ingest),
        JobTable::Graph => assert_contains(JobTable::Graph),
    };
    // Runtime: call assert_contains for every variant so omissions fail the test.
    assert_contains(JobTable::Crawl);
    assert_contains(JobTable::Refresh);
    assert_contains(JobTable::Extract);
    assert_contains(JobTable::Embed);
    assert_contains(JobTable::Ingest);
    assert_contains(JobTable::Graph);
    // Also check for accidental duplicates.
    let unique: std::collections::HashSet<&str> = all.iter().map(|t| t.as_str()).collect();
    assert_eq!(
        unique.len(),
        all.len(),
        "Duplicate table names detected in ALL_JOB_TABLES"
    );
}

#[test]
fn all_job_status_variants_covered() {
    // Verify each JobStatus variant is present in ALL_JOB_STATUSES.  The
    // exhaustive match closure below will fail to *compile* if a new variant
    // is added to the enum without being listed here, and the assert_contains
    // calls make the runtime test fail if the variant is in the match but was
    // accidentally omitted from the slice.
    let all: &[JobStatus] = &ALL_JOB_STATUSES;
    let assert_contains = |v: JobStatus| {
        assert!(
            all.contains(&v),
            "ALL_JOB_STATUSES is missing variant {v:?} — add it to the slice"
        );
    };
    // This closure is never called at runtime, but it *must* compile — the
    // exhaustive match forces a compile error when a new JobStatus variant is
    // introduced without updating both this match and the ALL_JOB_STATUSES slice.
    let _ = |s: JobStatus| match s {
        JobStatus::Pending => assert_contains(JobStatus::Pending),
        JobStatus::Running => assert_contains(JobStatus::Running),
        JobStatus::Completed => assert_contains(JobStatus::Completed),
        JobStatus::Failed => assert_contains(JobStatus::Failed),
        JobStatus::Canceled => assert_contains(JobStatus::Canceled),
    };
    // Runtime: call assert_contains for every variant so omissions fail the test.
    assert_contains(JobStatus::Pending);
    assert_contains(JobStatus::Running);
    assert_contains(JobStatus::Completed);
    assert_contains(JobStatus::Failed);
    assert_contains(JobStatus::Canceled);
    // Also check for accidental duplicates.
    let unique: std::collections::HashSet<&str> = all.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        unique.len(),
        all.len(),
        "Duplicate status values detected in ALL_JOB_STATUSES"
    );
}
