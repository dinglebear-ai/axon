//! Required schema comes from applied receipts, not future migration tails.
use super::*;
use sqlx::Connection;

pub(super) async fn validate_required_tables(
    connection: &mut SqliteConnection,
    sets: &[MigrationSet],
) -> Result<(), sqlx::Error> {
    let receipts: BTreeSet<(String, i64)> =
        sqlx::query("SELECT namespace, version FROM axon_applied_migrations")
            .fetch_all(&mut *connection)
            .await?
            .into_iter()
            .map(|row| (row.get("namespace"), row.get("version")))
            .collect();
    // Replay canonical DDL into a disposable connection. No production data or
    // pending migrations are touched; receipt identity was validated first.
    let mut baseline = SqliteConnection::connect(":memory:").await?;
    super::super::ensure_applied_table(&mut baseline).await?;
    for set in sets {
        for migration in set.migrations {
            if receipts.contains(&(set.namespace.to_string(), migration.version)) {
                baseline.execute(migration.sql).await?;
            }
        }
    }
    let expected = table_inventory(&mut baseline).await?;
    let actual = table_inventory(connection).await?;
    let missing: Vec<_> = expected.difference(&actual).collect();
    require(
        missing.is_empty(),
        format!("applied migration tables are missing: {missing:?}"),
    )
}

pub(super) async fn validate_table_subset(
    connection: &mut SqliteConnection,
) -> Result<(), sqlx::Error> {
    let actual = table_inventory(connection).await?;
    let canonical = canonical_table_inventory();
    let unknown = actual.difference(&canonical).cloned().collect::<Vec<_>>();
    require(
        unknown.is_empty(),
        format!("table inventory contains unknown tables: {unknown:?}"),
    )
}
