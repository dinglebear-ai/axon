-- Remove the retired session-specific watch store.
--
-- Sessions now run through the unified SourceRequest + source-watch pipeline.
-- Keeping these tables in the final schema makes the removed session-watch
-- service look like an active contract, so drop them at the cutover boundary.
DROP TABLE IF EXISTS axon_session_watch_errors;
DROP TABLE IF EXISTS axon_session_watch_checkpoints;
