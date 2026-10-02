use super::*;

#[test]
fn physical_stage_budget_allows_sqlite_overhead_and_counts_all_sidecars() {
    let directory =
        std::env::temp_dir().join(format!("axon-stage-size-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("stage.sqlite");
    let database = fs::File::create(&path).unwrap();
    let physical_limit = 4 * LIMIT;
    // Sparse files test the byte guard without writing or allocating 1 GiB.
    database.set_len(LIMIT + 1).unwrap();
    check_size(&path).expect("SQLite overhead may exceed the logical payload budget");
    database.set_len(physical_limit / 2).unwrap();
    let wal = fs::File::create(path.with_extension("sqlite-wal")).unwrap();
    wal.set_len(physical_limit / 2 - 2).unwrap();
    let shm = fs::File::create(path.with_extension("sqlite-shm")).unwrap();
    shm.set_len(1).unwrap();
    let journal = fs::File::create(path.with_extension("sqlite-journal")).unwrap();
    journal.set_len(1).unwrap();
    check_size(&path).expect("the exact combined physical limit is allowed");
    journal.set_len(2).unwrap();
    let error = check_size(&path).expect_err("all sidecars count toward the physical limit");
    assert_eq!(
        error.details.get("observed_bytes"),
        Some(&(physical_limit + 1).to_string())
    );
    assert_eq!(
        error.details.get("limit_bytes"),
        Some(&physical_limit.to_string())
    );
    drop((database, wal, shm, journal));
    fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn bulk_apply_skips_identical_rows_and_applies_nullable_changes() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::migration::ensure_schema(&pool).await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    sqlx::query("ATTACH DATABASE ':memory:' AS axon_stage")
        .execute(&mut *conn)
        .await
        .unwrap();
    for (table, _) in TABLES {
        let schema: String =
            sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type='table' AND name=?")
                .bind(table)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        let schema = schema.replacen(table, &format!("axon_stage.{table}"), 1);
        sqlx::query(&schema).execute(&mut *conn).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO axon_stage.graph_nodes VALUES ('n','repository','key','uri','name','derived',0.8,'{}','[]','created','updated');
        INSERT INTO axon_stage.graph_edges VALUES ('e','contains','n','n','derived',0.8,'{}','created','updated');
        INSERT INTO axon_stage.graph_aliases VALUES ('uri','alias','n');
        INSERT INTO axon_stage.graph_evidence VALUES ('ev','e','document','s','item',NULL,NULL,NULL,NULL,0.8,'{}');
        INSERT INTO axon_stage.graph_conflicts VALUES ('c','node','n','name','old','new','derived','derived','time');")
        .execute(&mut *conn).await.unwrap();
    bulk_apply(&mut conn).await.unwrap();
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM graph_revision")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    bulk_apply(&mut conn).await.unwrap();
    let unchanged: i64 = sqlx::query_scalar("SELECT revision FROM graph_revision")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(
        unchanged, revision,
        "identical seeded rows must not fire update triggers"
    );
    sqlx::raw_sql("UPDATE axon_stage.graph_evidence SET quote='changed'; UPDATE axon_stage.graph_nodes SET created_at='ignored';")
        .execute(&mut *conn).await.unwrap();
    bulk_apply(&mut conn).await.unwrap();
    let changed: i64 = sqlx::query_scalar("SELECT revision FROM graph_revision")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(changed, revision + 1);
    let quote: Option<String> = sqlx::query_scalar("SELECT quote FROM graph_evidence")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(quote.as_deref(), Some("changed"));
    sqlx::query("UPDATE axon_stage.graph_evidence SET quote=NULL")
        .execute(&mut *conn)
        .await
        .unwrap();
    bulk_apply(&mut conn).await.unwrap();
    let quote: Option<String> = sqlx::query_scalar("SELECT quote FROM graph_evidence")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(quote, None);
    let created: String = sqlx::query_scalar("SELECT created_at FROM graph_nodes")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(created, "created");
    sqlx::raw_sql("INSERT INTO axon_stage.graph_nodes SELECT 'n2',kind,'key2',canonical_uri,display_name,authority,confidence,metadata_json,source_ids_json,created_at,updated_at FROM axon_stage.graph_nodes WHERE node_id='n'; UPDATE axon_stage.graph_aliases SET node_id='n2';")
        .execute(&mut *conn).await.unwrap();
    bulk_apply(&mut conn).await.unwrap();
    let alias_node: String = sqlx::query_scalar("SELECT node_id FROM graph_aliases")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(
        alias_node, "n2",
        "changed alias mappings must still publish"
    );
}
