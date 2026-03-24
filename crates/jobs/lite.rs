pub mod cancel;
pub mod ops;
pub mod query;
pub mod store;
pub mod workers;

#[cfg(test)]
mod tests {
    use super::store::open_sqlite_pool;

    #[tokio::test]
    async fn sqlite_pool_opens_and_tables_exist() {
        let pool = open_sqlite_pool(":memory:")
            .await
            .expect("pool should open");

        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'axon_%_jobs'",
        )
        .fetch_one(&pool)
        .await
        .expect("sqlite_master query should work");
        assert_eq!(row.0, 6, "expected 6 job tables");
    }
}
