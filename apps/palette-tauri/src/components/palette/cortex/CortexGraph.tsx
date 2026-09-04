import type { CortexGraphResult } from "@/lib/clients/cortexClient";
import { safeText } from "@/lib/cortex/viewModel";

export function CortexGraph({ graph }: { graph: CortexGraphResult }) {
  return (
    <section aria-label="Cortex graph" className="cortex-graph">
      <p>
        {graph.projection_status ?? "Projection status unavailable"} · watermark{" "}
        {safeText(graph.source_watermark ?? "unknown", 120)}
      </p>
      {graph.degraded_reason && (
        <p role="status">Degraded: {safeText(graph.degraded_reason, 500)}</p>
      )}
      {graph.relationships?.slice(0, 100).map((edge) => (
        <article key={edge.id}>
          <h2>{safeText(edge.relationship_type, 120)}</h2>
          <p>
            {safeText(edge.src_entity?.display_label ?? edge.src_entity?.canonical_key)} →{" "}
            {safeText(edge.dst_entity?.display_label ?? edge.dst_entity?.canonical_key)}
          </p>
          <small>
            {edge.evidence_count} evidence records · {Math.round(edge.confidence * 100)}% confidence
          </small>
        </article>
      ))}
    </section>
  );
}
