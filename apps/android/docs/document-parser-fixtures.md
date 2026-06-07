# Android Document Parser Fixtures

`DocumentParsing.kt` is a presentation parser for Axon retrieve output, not a
general Markdown engine. Keep fixture coverage for these output classes before
changing its regex heuristics:

- Normal Markdown headings, body paragraphs, bullets, and fenced code blocks.
- Axon/OpenAI schema dumps that include `(resource)`, `(schema)`, anchor
  fragments, `object { ... }` field summaries, and `JSON string` argument copy.
- Sitemap-like retrieve output with many URLs plus frequency/priority metadata.
- Long generated paragraphs that need stable card-sized splitting.
- Retrieve warnings where truncation copy is already represented elsewhere in
  the UI.

Representative tests live in
`app/src/test/java/com/axon/app/ui/document/DocumentChunkingTest.kt`.
