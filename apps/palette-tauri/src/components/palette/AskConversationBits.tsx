import { Brain, CheckCircle2, Wrench } from "lucide-react";

import type { AskActivity, AskSource } from "@/lib/runState";

export function SourceStrip({ sources }: { sources?: AskSource[] }) {
  if (!sources?.length) return null;
  return (
    <details className="ask-sources">
      <summary>Sources</summary>
      <div>
        {sources.map((source, index) =>
          source.url ? (
            <a key={source.url} href={source.url} target="_blank" rel="noreferrer">
              <span>{index + 1}</span>
              {source.label}
            </a>
          ) : (
            <span key={`${source.label}:${source.title ?? ""}`}>
              <span>{index + 1}</span>
              {source.label}
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
  return (
    <section className="ask-activity" aria-label={pending ? "Agent activity" : "Agent activity summary"}>
      {activities.map((activity) => (
        <div key={activity.id} className={`ask-activity-row ask-activity-${activity.kind ?? "thinking"}`}>
          <ActivityIcon activity={activity} />
          <span>
            <strong>{activity.label}</strong>
            {activity.detail ? <small>{activity.detail}</small> : null}
          </span>
        </div>
      ))}
    </section>
  );
}

function ActivityIcon({ activity }: { activity: AskActivity }) {
  if (activity.kind === "tool") return <Wrench size={12} strokeWidth={1.8} aria-hidden="true" />;
  if (activity.kind === "done") return <CheckCircle2 size={12} strokeWidth={1.8} aria-hidden="true" />;
  return <Brain size={12} strokeWidth={1.8} aria-hidden="true" />;
}
