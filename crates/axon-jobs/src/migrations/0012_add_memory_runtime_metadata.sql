-- Store runtime context captured when manual memories are created.

ALTER TABLE axon_memory_nodes ADD COLUMN workspace TEXT;
ALTER TABLE axon_memory_nodes ADD COLUMN git_branch TEXT;
ALTER TABLE axon_memory_nodes ADD COLUMN git_commit TEXT;
ALTER TABLE axon_memory_nodes ADD COLUMN git_dirty INTEGER;
ALTER TABLE axon_memory_nodes ADD COLUMN cwd TEXT;
