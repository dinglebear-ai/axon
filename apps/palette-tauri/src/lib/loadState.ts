// Shared async-load state shape, used by structured result views that fetch
// data after mount (GitHubView's file/tree panes, GitHubFeedView's feed
// fetch). Consolidated here after `GitHubView.tsx` and `GitHubFeedView.tsx`
// had drifted to two near-identical-but-not-identical local definitions
// (`GitHubFeedView`'s was missing the `idle` variant and used a `payload`
// field where `GitHubView`'s used `value`) — see PR review discussion on
// palette-github-enhancements.

export type LoadState<T> =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "loaded"; value: T }
  | { kind: "error"; message: string };
