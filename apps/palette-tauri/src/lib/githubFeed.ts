// Cross-repo activity Feed — types and pure presentation helpers.
//
// `FeedItem` mirrors `src-tauri/src/github_feed.rs::FeedItem` field-for-field
// (camelCase because that Rust struct is `#[serde(rename_all = "camelCase")]`,
// same convention as `GitHubBrowseResult` in actionRequest.ts). Grouping by
// day happens here, not in Rust — it's a presentation concern (depends on the
// viewer's "now," which the bridge has no reason to know about), matching how
// the reference mock also grouped its (fixture) feed data client-side.
//
// Data source: GitHub's Events API, fanned out per-repo by the Rust bridge —
// see docs/plans/palette-github-enhancements.md's "Data source decision"
// section for why this was chosen over the Notifications API.
//
// Kind taxonomy verified against palette-mock.html's real `var FEED_KIND =
// {...}` object: pr/merge/review/comment/conflict/deps/issue/push/release.
// `comment` and `conflict` are registered here for forward-compatibility but
// `github_feed.rs::normalize_event` never emits them in this pass (no clean
// Events API source — see Task 3's "Mock-verified taxonomy" note).

import {
  AlertTriangle,
  GitBranch,
  GitCommit,
  GitMerge,
  GitPullRequest,
  MessageSquare,
  Package,
  Tag,
  type LucideIcon,
} from "lucide-react";

// Only the status-label case is ever constructed by the Rust normalizer
// (`github_feed.rs::normalize_event`) — the Events API has no single-call
// additions/deletions source for any event type this plan sources, so the
// `{ type: "diff"; add; del }` case was removed rather than kept as an
// unproducible union member. Reintroduce it if a future normalizer gains a
// reliable diff-stat source.
export type FeedBadge = { type: "label"; value: string };

export interface FeedItem {
  /** One of: "pr" | "merge" | "review" | "comment" | "conflict" | "deps" | "issue" | "push" | "release". */
  kind: string;
  /** `owner/repo`. */
  repo: string;
  actor: string;
  title: string;
  url: string;
  path: string | null;
  /** PR/issue number, or `null` for kinds that don't carry one (push/release). */
  num: number | null;
  /** Short freeform descriptive line — see `github_feed.rs::FeedItem::meta` doc comment for examples per kind. */
  meta: string;
  /** Line-diff or status-label badge, or `null` when this event type has no reliable single-call source. */
  badge: FeedBadge | null;
  timestampUnix: number;
}

export interface FeedPayload {
  items: FeedItem[];
  partial: boolean;
  errors: string[];
}

export interface FeedDayGroup {
  label: "Today" | "Yesterday" | "Earlier";
  items: FeedItem[];
}

/**
 * Group feed items into Today/Yesterday/Earlier buckets relative to `nowMs`,
 * matching the mock's day-grouping. Empty buckets are omitted. Within each
 * bucket, item order is preserved from the input (callers should pass
 * already-sorted-descending items — the Rust bridge already sorts by
 * `timestamp_unix` descending before returning).
 */
export function groupFeedByDay(items: FeedItem[], nowMs: number = Date.now()): FeedDayGroup[] {
  const now = new Date(nowMs);
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const startOfYesterday = startOfToday - 24 * 60 * 60 * 1000;

  const today: FeedItem[] = [];
  const yesterday: FeedItem[] = [];
  const earlier: FeedItem[] = [];

  for (const item of items) {
    const ms = item.timestampUnix * 1000;
    if (ms >= startOfToday) {
      today.push(item);
    } else if (ms >= startOfYesterday) {
      yesterday.push(item);
    } else {
      earlier.push(item);
    }
  }

  const groups: FeedDayGroup[] = [];
  if (today.length > 0) groups.push({ label: "Today", items: today });
  if (yesterday.length > 0) groups.push({ label: "Yesterday", items: yesterday });
  if (earlier.length > 0) groups.push({ label: "Earlier", items: earlier });
  return groups;
}

// Labels verified against palette-mock.html's real FEED_KIND object — note
// "Pull Request" is capital-R (the mock, not "Pull request" from the first
// drafting pass) and there is no "dependency-bump" kind; the mock's actual
// dependency kind is "deps" labeled "Dependencies".
const FEED_KIND_LABELS: Record<string, string> = {
  pr: "Pull Request",
  merge: "Merged",
  review: "Review",
  comment: "Comment",
  conflict: "Conflict",
  deps: "Dependencies",
  issue: "Issue",
  push: "Push",
  release: "Release",
};

export function feedKindLabel(kind: string): string {
  return FEED_KIND_LABELS[kind] ?? kind;
}

// Icon substitutes for the mock's inline SVG glyphs (the mock ships a
// hand-drawn path per kind via `Svg(k.g, 15)`; this plan uses the closest
// lucide-react icon per kind rather than porting raw SVG paths).
const FEED_KIND_ICONS: Record<string, LucideIcon> = {
  pr: GitPullRequest,
  merge: GitMerge,
  review: GitBranch,
  comment: MessageSquare,
  conflict: AlertTriangle,
  deps: Package,
  issue: GitBranch,
  push: GitCommit,
  release: Tag,
};

export function feedKindIcon(kind: string): LucideIcon {
  return FEED_KIND_ICONS[kind] ?? GitCommit;
}
