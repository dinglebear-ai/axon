use chrono::{DateTime, Utc};

pub(crate) fn list_status_rank(status: &str) -> u8 {
    match status {
        "running" => 0,
        "pending" => 1,
        "completed" => 2,
        "failed" => 3,
        "canceled" => 4,
        _ => 5,
    }
}

pub(crate) fn sort_rows_for_status_view<T, FStatus, FCreated, FUpdated>(
    rows: &mut [T],
    status_of: FStatus,
    created_of: FCreated,
    updated_of: FUpdated,
) where
    FStatus: Fn(&T) -> &str,
    FCreated: Fn(&T) -> &DateTime<Utc>,
    FUpdated: Fn(&T) -> &DateTime<Utc>,
{
    rows.sort_by(|a, b| {
        list_status_rank(status_of(a))
            .cmp(&list_status_rank(status_of(b)))
            .then_with(|| created_of(b).cmp(created_of(a)))
            .then_with(|| updated_of(b).cmp(updated_of(a)))
            .then_with(|| status_of(a).cmp(status_of(b)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[derive(Clone, Debug)]
    struct Row {
        status: &'static str,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        label: &'static str,
    }

    #[test]
    fn sorts_active_first_then_recent_then_failures() {
        let mut rows = vec![
            Row {
                status: "failed",
                created_at: Utc.with_ymd_and_hms(2026, 3, 18, 10, 0, 0).unwrap(),
                updated_at: Utc.with_ymd_and_hms(2026, 3, 18, 10, 1, 0).unwrap(),
                label: "failed-new",
            },
            Row {
                status: "completed",
                created_at: Utc.with_ymd_and_hms(2026, 3, 18, 10, 2, 0).unwrap(),
                updated_at: Utc.with_ymd_and_hms(2026, 3, 18, 10, 2, 5).unwrap(),
                label: "completed-new",
            },
            Row {
                status: "running",
                created_at: Utc.with_ymd_and_hms(2026, 3, 18, 10, 3, 0).unwrap(),
                updated_at: Utc.with_ymd_and_hms(2026, 3, 18, 10, 3, 5).unwrap(),
                label: "running",
            },
            Row {
                status: "pending",
                created_at: Utc.with_ymd_and_hms(2026, 3, 18, 10, 4, 0).unwrap(),
                updated_at: Utc.with_ymd_and_hms(2026, 3, 18, 10, 4, 5).unwrap(),
                label: "pending",
            },
        ];

        sort_rows_for_status_view(
            &mut rows,
            |r| r.status,
            |r| &r.created_at,
            |r| &r.updated_at,
        );

        let labels: Vec<&str> = rows.iter().map(|r| r.label).collect();
        assert_eq!(
            labels,
            vec!["running", "pending", "completed-new", "failed-new"]
        );
    }
}
