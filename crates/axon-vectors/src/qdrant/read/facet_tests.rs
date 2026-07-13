use super::*;

#[test]
fn sorts_by_value_ascending() {
    let hits = vec![
        FacetHit {
            value: Some("zeta.example.com".to_string()),
            count: Some(3),
        },
        FacetHit {
            value: Some("alpha.example.com".to_string()),
            count: Some(5),
        },
    ];
    let out = parse_facet_hits(hits);
    assert_eq!(
        out,
        vec![
            ("alpha.example.com".to_string(), 5),
            ("zeta.example.com".to_string(), 3),
        ]
    );
}

#[test]
fn missing_value_falls_back_to_unknown() {
    let hits = vec![FacetHit {
        value: None,
        count: Some(1),
    }];
    let out = parse_facet_hits(hits);
    assert_eq!(out, vec![("unknown".to_string(), 1)]);
}

#[test]
fn empty_string_value_is_dropped() {
    let hits = vec![
        FacetHit {
            value: Some(String::new()),
            count: Some(9),
        },
        FacetHit {
            value: Some("kept.example.com".to_string()),
            count: Some(1),
        },
    ];
    let out = parse_facet_hits(hits);
    assert_eq!(out, vec![("kept.example.com".to_string(), 1)]);
}

#[test]
fn missing_count_defaults_to_zero() {
    let hits = vec![FacetHit {
        value: Some("no-count.example.com".to_string()),
        count: None,
    }];
    let out = parse_facet_hits(hits);
    assert_eq!(out, vec![("no-count.example.com".to_string(), 0)]);
}
