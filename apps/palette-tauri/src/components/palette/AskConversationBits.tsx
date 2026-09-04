import {
  Brain,
  CheckCircle2,
  ExternalLink,
  ShieldAlert,
  TriangleAlert,
  Wrench,
} from "lucide-react";

import type { AskActivity, AskSource } from "@/lib/runState";

export function SourceStrip({ sources }: { sources?: AskSource[] }) {
  if (!sources?.length) return null;
  return (
    <details className="ask-sources">
      <summary>Sources</summary>
      <div className="ask-source-list">
        {sources.map((source, index) =>
          source.url ? (
            <a
              key={source.url}
              href={source.url}
              target="_blank"
              rel="noreferrer"
              className="ask-source-card"
            >
              <span className="ask-source-rank">{index + 1}</span>
              <span className="ask-source-copy">
                <strong>{source.title ?? source.label}</strong>
                {source.title && source.title !== source.label ? (
                  <small>{source.label}</small>
                ) : null}
                {source.snippet ? <p>{source.snippet}</p> : null}
              </span>
              {source.score !== undefined ? <em>{source.score.toFixed(2)}</em> : null}
              <ExternalLink size={12} aria-hidden="true" />
            </a>
          ) : (
            <span key={`${source.label}:${source.title ?? ""}`} className="ask-source-card">
              <span className="ask-source-rank">{index + 1}</span>
              <span className="ask-source-copy">
                <strong>{source.title ?? source.label}</strong>
                {source.snippet ? <p>{source.snippet}</p> : null}
              </span>
            </span>
          ),
        )}
      </div>
    </details>
  );
}

export function ActivityTrail({
  activities,
  pending,
}: {
  activities?: AskActivity[];
  pending?: boolean;
}) {
  if (!activities?.length) return null;
  const rows = activities.map((activity) => (
    <div
      key={activity.id}
      className={`ask-activity-row ask-activity-${activity.kind ?? "thinking"}`}
    >
      <ActivityIcon activity={activity} />
      <span>
        <strong>{activity.label}</strong>
        {activity.detail ? <small>{activity.detail}</small> : null}
      </span>
    </div>
  ));
  if (!pending) {
    return (
      <details className="ask-activity ask-activity-collapsed">
        <summary>
          {activities.length} activity {activities.length === 1 ? "step" : "steps"}
        </summary>
        {rows}
      </details>
    );
  }
  return (
    <section
      className="ask-activity"
      aria-label={pending ? "Agent activity" : "Agent activity summary"}
    >
      {rows}
    </section>
  );
}

function ActivityIcon({ activity }: { activity: AskActivity }) {
  if (activity.kind === "tool") return <Wrench size={12} strokeWidth={1.8} aria-hidden="true" />;
  if (activity.kind === "approval")
    return <ShieldAlert size={12} strokeWidth={1.8} aria-hidden="true" />;
  if (activity.kind === "warning")
    return <TriangleAlert size={12} strokeWidth={1.8} aria-hidden="true" />;
  if (activity.kind === "done")
    return <CheckCircle2 size={12} strokeWidth={1.8} aria-hidden="true" />;
  return <Brain size={12} strokeWidth={1.8} aria-hidden="true" />;
}
