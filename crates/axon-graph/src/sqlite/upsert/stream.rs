//! Bounded transactions for the lazy production graph publication path.

use super::*;

const MAX_NODES: usize = 512;
const MAX_EDGES: usize = 256;
const MAX_QUOTE_BYTES: usize = 64 * 1024;

#[derive(Default)]
struct Weight {
    nodes: usize,
    edges: usize,
    quotes: usize,
}

impl Weight {
    fn add(&mut self, candidate: &GraphCandidate) {
        self.nodes = self.nodes.saturating_add(candidate.nodes.len());
        self.edges = self.edges.saturating_add(candidate.edges.len());
        self.quotes = self.quotes.saturating_add(
            candidate
                .evidence
                .iter()
                .map(|ev| ev.quote.as_ref().map_or(0, String::len))
                .sum::<usize>(),
        );
    }

    fn full(&self) -> bool {
        self.nodes >= MAX_NODES || self.edges >= MAX_EDGES || self.quotes >= MAX_QUOTE_BYTES
    }
}

pub async fn upsert_candidate_iter<I>(
    pool: &SqlitePool,
    write_gate: &axon_core::sqlite::SqliteWriteGate,
    candidates: I,
) -> StoreResult<GraphWriteResult>
where
    I: IntoIterator<Item = GraphCandidate>,
{
    let mut source_id: Option<SourceId> = None;
    let mut totals = CandidateCounts::default();
    let mut pending = Vec::with_capacity(CANDIDATE_TRANSACTION_BATCH_SIZE);
    let mut weight = Weight::default();
    for candidate in candidates {
        let validation = validate_candidate(&candidate).and_then(|()| {
            if source_id
                .as_ref()
                .is_some_and(|source| source != &candidate.source_id)
            {
                Err(graph_validation_error(
                    "mixed-source graph batches require per-source receipts",
                ))
            } else {
                Ok(())
            }
        });
        if let Err(error) = validation {
            // Streaming callers retain the valid prefix even for validation errors.
            flush(pool, write_gate, &mut pending, &mut totals).await?;
            return Err(error);
        }
        source_id.get_or_insert_with(|| candidate.source_id.clone());
        let mut next_weight = Weight {
            nodes: weight.nodes,
            edges: weight.edges,
            quotes: weight.quotes,
        };
        next_weight.add(&candidate);
        if next_weight.full() && !pending.is_empty() {
            flush(pool, write_gate, &mut pending, &mut totals).await?;
            weight = Weight::default();
        }
        weight.add(&candidate);
        pending.push(candidate);
        // An oversized candidate stays atomic, but is never grouped with others.
        if pending.len() == CANDIDATE_TRANSACTION_BATCH_SIZE || weight.full() {
            flush(pool, write_gate, &mut pending, &mut totals).await?;
            weight = Weight::default();
        }
    }
    flush(pool, write_gate, &mut pending, &mut totals).await?;
    Ok(totals.result(source_id))
}

async fn flush(
    pool: &SqlitePool,
    gate: &axon_core::sqlite::SqliteWriteGate,
    pending: &mut Vec<GraphCandidate>,
    totals: &mut CandidateCounts,
) -> StoreResult<()> {
    if !pending.is_empty() {
        let result = upsert_candidates(pool, gate, std::mem::take(pending)).await?;
        totals.add(CandidateCounts {
            seen: result.candidates_seen,
            nodes: result.nodes_upserted,
            edges: result.edges_upserted,
            evidence: result.evidence_records,
        });
    }
    Ok(())
}
