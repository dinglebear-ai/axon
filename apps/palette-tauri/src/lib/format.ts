// Output classification and fallback-text formatting derive from the per-action
// registry (finding A-H1). Keep these public signatures stable: App.tsx,
// useActionRunner, useCrawlJob, and historyRun import them directly.

import { actionBehavior, type OutputKind } from "./actionRegistry";

export type { OutputKind };

/**
 * Minimum visible width (%) for a progress bar so a just-started job still shows
 * a sliver. Consumed by the App.tsx idle tray and CrawlJobView.
 */
export const MIN_PROGRESS_PCT = 2;

export function outputKindFor(subcommand: string): OutputKind {
  return actionBehavior(subcommand).outputKind;
}

export function formatPayload(subcommand: string, payload: unknown): string {
  return actionBehavior(subcommand).formatText(payload);
}
