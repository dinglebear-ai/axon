import { Activity, RotateCw, X, type LucideIcon } from "lucide-react";

import type { ActionBehavior } from "./actionRegistry";
import type { components } from "./axon-api";
import { JOB_OPERATIONS, type JobOperation, type JobSubcommand } from "./actions";
import { type ActionRouteTemplate, type BodyBuilder, deleteRoute, first, getRoute, noBody, postRoute, uuid } from "./actionRequest";
import { formatJobLifecycle, recordFormatter } from "./actionFormat";

type Req = components["schemas"];

function lifecycleBehavior(operation: JobOperation): ActionBehavior {
  return {
    route: lifecycleRoute(operation),
    buildBody: lifecycleBody(operation),
    routeFor: lifecycleRouteFor(operation),
    outputKind: "code",
    formatText: recordFormatter(formatJobLifecycle),
    actionIcon: Activity,
    outputIcon: lifecycleOutputIcon(operation),
    structuredView: "job-lifecycle",
  };
}

function lifecycleRoute(operation: JobOperation): ActionRouteTemplate {
  switch (operation) {
    case "list":
      return getRoute(`/v1/jobs`);
    case "status":
      return getRoute(`/v1/jobs/{id}`);
    case "cancel":
      return postRoute(`/v1/jobs/{id}/cancel`);
    case "cleanup":
      return postRoute(`/v1/jobs/cleanup`);
    case "clear":
      return deleteRoute(`/v1/jobs`);
    case "recover":
      return postRoute(`/v1/jobs/recover`);
  }
}

function lifecycleRouteFor(operation: JobOperation): ActionBehavior["routeFor"] {
  switch (operation) {
    case "status":
      return (ctx) => getRoute(`/v1/jobs/${uuid(first(ctx.words, "job id"))}`);
    case "cancel":
      return (ctx) => postRoute(`/v1/jobs/${uuid(first(ctx.words, "job id"))}/cancel`);
    default:
      return undefined;
  }
}

function lifecycleBody(operation: JobOperation): BodyBuilder {
  switch (operation) {
    case "list":
    case "status":
      return noBody;
    case "cancel":
      return (() => ({})) satisfies BodyBuilder<Req["JobCancelRequest"]>;
    case "cleanup":
      return (() => ({ dry_run: false })) satisfies BodyBuilder<Req["JobCleanupRequest"]>;
    case "clear":
      return (() => ({ confirm: true })) satisfies BodyBuilder<Req["JobClearRequest"]>;
    case "recover":
      return (() => ({})) satisfies BodyBuilder<Req["JobRecoveryRequest"]>;
  }
}

function lifecycleOutputIcon(operation: JobOperation): LucideIcon {
  switch (operation) {
    case "cancel":
      return X;
    case "cleanup":
    case "clear":
    case "recover":
      return RotateCw;
    case "list":
    case "status":
      return Activity;
  }
}

export function buildLifecycleRegistry(): Record<JobSubcommand, ActionBehavior> {
  const out = {} as Record<JobSubcommand, ActionBehavior>;
  for (const operation of JOB_OPERATIONS) {
    out[`jobs-${operation}`] = lifecycleBehavior(operation);
  }
  return out;
}
