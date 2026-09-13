use super::*;

#[test]
fn parses_prometheus_metric_lines() {
    let metrics = parse_prometheus_metrics(
        r#"
# TYPE te_request_failure counter
te_request_failure{err="overloaded"} 12
te_embed_count 42
te_request_input_length_sum 8192
te_request_input_length_count 16
rest_responses_total{method="PUT",endpoint="/collections/{collection_name}/points",status="200"} 3
"#,
    );

    assert_eq!(
        metrics.get(r#"te_request_failure{err="overloaded"}"#),
        Some(&12.0)
    );
    assert_eq!(metrics.get("te_embed_count"), Some(&42.0));
    assert_eq!(metrics.get("te_request_input_length_sum"), Some(&8192.0));
    assert_eq!(metrics.get("te_request_input_length_count"), Some(&16.0));
    assert_eq!(
            metrics.get(
                r#"rest_responses_total{method="PUT",endpoint="/collections/{collection_name}/points",status="200"}"#
            ),
            Some(&3.0)
        );
}
