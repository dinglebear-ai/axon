use super::*;

#[test]
fn new_returns_none_when_url_empty() {
    let client = Neo4jClient::from_parts("", "neo4j", "").unwrap();
    assert!(client.is_none());
}

#[test]
fn new_returns_some_when_url_set() {
    let client = Neo4jClient::from_parts("http://localhost:7474", "neo4j", "pass").unwrap();
    assert!(client.is_some());
}

#[test]
fn rejects_non_http_scheme() {
    match Neo4jClient::from_parts("bolt://localhost:7687", "neo4j", "pass") {
        Ok(_) => panic!("bolt scheme must be rejected"),
        Err(err) => assert!(
            err.to_string()
                .contains("AXON_NEO4J_URL must use http:// or https://")
        ),
    }
}

#[test]
fn build_request_body_single_statement() {
    let body = build_request_body("RETURN 1", serde_json::json!({}));
    let stmts = body["statements"].as_array().unwrap();
    assert_eq!(stmts.len(), 1);
    assert_eq!(stmts[0]["statement"], "RETURN 1");
}

#[test]
fn build_request_body_with_params() {
    let params = serde_json::json!({"name": "Tokio"});
    let body = build_request_body("MATCH (e:Entity {name: $name}) RETURN e", params.clone());
    assert_eq!(body["statements"][0]["parameters"], params);
}

#[test]
fn auth_header_built_correctly() {
    let client = Neo4jClient::from_parts("http://localhost:7474", "neo4j", "secret")
        .unwrap()
        .unwrap();
    assert_eq!(client.endpoint, "http://localhost:7474/db/neo4j/tx/commit");
}
