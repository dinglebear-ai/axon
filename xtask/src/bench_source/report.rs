use super::*;

pub(super) fn summarize_records(records: &[RunRecord]) -> anyhow::Result<Vec<ScenarioSummary>> {
    let mut grouped = BTreeMap::<String, Vec<&RunRecord>>::new();
    for record in records {
        grouped
            .entry(record.scenario.clone())
            .or_default()
            .push(record);
    }
    grouped
        .into_iter()
        .map(|(scenario, runs)| {
            let mut phases = BTreeMap::<String, Vec<f64>>::new();
            let mut warnings = BTreeMap::new();
            for run in &runs {
                for (phase, seconds) in &run.phase_seconds {
                    phases.entry(phase.clone()).or_default().push(*seconds);
                }
                for (code, count) in &run.warning_codes {
                    *warnings.entry(code.clone()).or_insert(0) += count;
                }
            }
            let point_values = runs
                .iter()
                .map(|run| run.points.map(|value| value as f64))
                .collect::<Option<Vec<_>>>();
            Ok(ScenarioSummary {
                scenario,
                runs: runs.len(),
                wall_seconds: distribution(
                    &runs.iter().map(|run| run.wall_seconds).collect::<Vec<_>>(),
                )?,
                items: distribution(&runs.iter().map(|run| run.items as f64).collect::<Vec<_>>())?,
                documents: distribution(
                    &runs
                        .iter()
                        .map(|run| run.documents as f64)
                        .collect::<Vec<_>>(),
                )?,
                chunks: distribution(
                    &runs.iter().map(|run| run.chunks as f64).collect::<Vec<_>>(),
                )?,
                points: point_values.as_deref().map(distribution).transpose()?,
                phase_median_seconds: phases
                    .into_iter()
                    .map(|(phase, values)| Ok((phase, distribution(&values)?.median)))
                    .collect::<anyhow::Result<_>>()?,
                unattributed_seconds: distribution(
                    &runs
                        .iter()
                        .map(|run| run.unattributed_seconds)
                        .collect::<Vec<_>>(),
                )?,
                degraded_runs: runs
                    .iter()
                    .filter(|run| run.status == "completed_degraded")
                    .count(),
                warning_codes: warnings,
                retry_events: runs.iter().map(|run| run.retry_events).sum(),
            })
        })
        .collect()
}

pub(super) fn compare_summaries(
    baseline: &[ScenarioSummary],
    candidate: &[ScenarioSummary],
) -> Vec<ScenarioComparison> {
    candidate
        .iter()
        .filter_map(|current| {
            let prior = baseline
                .iter()
                .find(|prior| prior.scenario == current.scenario)?;
            Some(ScenarioComparison {
                scenario: current.scenario.clone(),
                wall_median_percent: percent_change(
                    prior.wall_seconds.median,
                    current.wall_seconds.median,
                ),
                items_median_percent: percent_change(prior.items.median, current.items.median),
                documents_median_percent: percent_change(
                    prior.documents.median,
                    current.documents.median,
                ),
                chunks_median_percent: percent_change(prior.chunks.median, current.chunks.median),
                points_median_percent: prior
                    .points
                    .as_ref()
                    .zip(current.points.as_ref())
                    .and_then(|(prior, current)| percent_change(prior.median, current.median)),
                phase_median_percent: current
                    .phase_median_seconds
                    .iter()
                    .filter_map(|(phase, current_seconds)| {
                        let prior_seconds = prior.phase_median_seconds.get(phase)?;
                        percent_change(*prior_seconds, *current_seconds)
                            .map(|change| (phase.clone(), change))
                    })
                    .collect(),
                degraded_runs_delta: current.degraded_runs as i64 - prior.degraded_runs as i64,
            })
        })
        .collect()
}

pub(super) fn print_human(report: &BenchReport, output: &Path) {
    println!("Live source benchmark complete");
    println!("site: {}", report.url);
    for summary in &report.summaries {
        println!(
            "{}: median {:.3}s (min {:.3}s, max {:.3}s), median {} pages / {} chunks",
            summary.scenario,
            summary.wall_seconds.median,
            summary.wall_seconds.min,
            summary.wall_seconds.max,
            summary.items.median,
            summary.chunks.median
        );
    }
    for comparison in &report.comparison {
        if let Some(change) = comparison.wall_median_percent {
            println!(
                "{} vs baseline: wall time {change:+.1}%",
                comparison.scenario
            );
        }
        if let Some(change) = comparison.points_median_percent {
            println!(
                "{} vs baseline: vector points {change:+.1}%",
                comparison.scenario
            );
        }
    }
    println!("artifact: {}", output.display());
}
