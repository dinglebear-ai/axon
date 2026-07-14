use anyhow::bail;

const ALLOWED_LABELS: &[&str] = &[
    "phase",
    "source_kind",
    "scope",
    "adapter",
    "status",
    "error_code",
    "provider_kind",
];

pub fn record_source_phase_with_labels(phase: &str, labels: &[(&str, &str)]) -> anyhow::Result<()> {
    validate_label("phase")?;
    let mut metric_labels = vec![("phase".to_string(), phase.to_string())];
    for (key, value) in labels {
        validate_label(key)?;
        if *key == "phase" {
            continue;
        }
        metric_labels.push(((*key).to_string(), (*value).to_string()));
    }
    metrics::counter!("axon_source_phase_total", &metric_labels).increment(1);
    Ok(())
}

fn validate_label(key: &str) -> anyhow::Result<()> {
    if !ALLOWED_LABELS.contains(&key) {
        bail!("unsupported source metric label `{key}`");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_metrics_accept_bounded_labels() {
        super::record_source_phase_with_labels(
            "fetching",
            &[
                ("source_kind", "web"),
                ("scope", "page"),
                ("adapter", "web"),
                ("status", "running"),
            ],
        )
        .expect("bounded labels accepted");
    }

    #[test]
    fn source_metrics_reject_high_cardinality_labels() {
        let err = super::record_source_phase_with_labels(
            "fetching",
            &[("url", "https://secret.example.test/token")],
        )
        .expect_err("url label rejected");
        assert!(err.to_string().contains("unsupported source metric label"));
    }
}
