# Evaluate Golden Set

This directory contains the stable golden questions used for `axon evaluate`
quality checks and backend parity reports.

`ask-golden.jsonl` is intentionally small and representative rather than
exhaustive. Each row has a stable `id`, `question`, `category`, and
`expected_traits` list. The set covers:

- short factual lookup
- natural-language RAG question
- multi-clause question
- code-related question
- source-sensitive question

Update rules:

- Keep IDs stable once published.
- Do not reuse an ID for a different question.
- Add new questions as new IDs.
- Reject duplicate IDs and blank questions in any report loader.
- Record the actual judge backend/model used by each parity row.

`axon evaluate --json` emits a `scores` object with `status`, per-axis `axes`,
`rag_total`, `baseline_total`, and `winner`. Report consumers must treat
`parse_failed` and `partial` as explicit statuses, not as zero scores.
