import { Activity, Braces, GitBranch, Layers, PackageOpen, RotateCw, X, type LucideIcon } from "lucide-react";

import type { ActionBehavior } from "./actionRegistry";
import { JOB_FAMILIES, JOB_OPERATIONS, type JobFamily, type JobOperation } from "./actions";
import { type ActionRouteTemplate, deleteRoute, first, getRoute, noBody, postRoute, uuid } from "./actionRequest";
import { formatJobLifecycle, recordFormatter } from "./actionFormat";

const JOB_LIFECYCLE_ICONS: Record<JobFamily, LucideIcon> = {
  crawl: GitBranch,
  embed: Layers,
  extract: Braces,
  ingest: PackageOpen,
};

function lifecycleBehavior(family: JobFamily, operation: JobOperation): ActionBehavior {
  const icon = JOB_LIFECYCLE_ICONS[family];
  return {
    route: lifecycleRoute(family, operation),
    buildBody: noBody,
    routeFor: lifecycleRouteFor(family, operation),
    outputKind: "code",
    formatText: recordFormatter(formatJobLifecycle),
    actionIcon: icon,
    outputIcon: lifecycleOutputIcon(family, operation),
    structuredView: "job-lifecycle",
  };
}

function lifecycleRoute(family: JobFamily, operation: JobOperation): ActionRouteTemplate {
  switch (operation) {
    case "list":
      return getRoute(`/v1/${family}`);
    case "status":
      return getRoute(`/v1/${family}/{id}`);
    case "cancel":
      return postRoute(`/v1/${family}/{id}/cancel`);
    case "cleanup":
      return postRoute(`/v1/${family}/cleanup`);
    case "clear":
      return deleteRoute(`/v1/${family}`);
    case "recover":
      return postRoute(`/v1/${family}/recover`);
  }
}

function lifecycleRouteFor(family: JobFamily, operation: JobOperation): ActionBehavior["routeFor"] {
  switch (operation) {
    case "status":
      return (ctx) => getRoute(`/v1/${family}/${uuid(first(ctx.words, "job id"))}`);
    case "cancel":
      return (ctx) => postRoute(`/v1/${family}/${uuid(first(ctx.words, "job id"))}/cancel`);
    default:
      return undefined;
  }
}

function lifecycleOutputIcon(family: JobFamily, operation: JobOperation): LucideIcon {
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
    default:
      return JOB_LIFECYCLE_ICONS[family];
  }
}

export function buildLifecycleRegistry(): Record<`${JobFamily}-${JobOperation}`, ActionBehavior> {
  const out = {} as Record<`${JobFamily}-${JobOperation}`, ActionBehavior>;
  for (const family of JOB_FAMILIES) {
    for (const operation of JOB_OPERATIONS) {
      out[`${family}-${operation}`] = lifecycleBehavior(family, operation);
    }
  }
  return out;
}
