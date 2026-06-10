-- Filesystem-namespace claim affinity for embed jobs (axon_rust-p2oc).
--
-- The shared jobs DB is polled by workers in different filesystem namespaces
-- (host CLI `--wait` runs vs the axon container). A path-like embed input is
-- only readable inside the enqueuer's namespace; fs_namespace records that
-- namespace at enqueue time so claims have affinity. NULL = no affinity
-- (URL / free-text inputs, plus all pre-migration rows) — claimable anywhere.
ALTER TABLE axon_embed_jobs ADD COLUMN fs_namespace TEXT;
