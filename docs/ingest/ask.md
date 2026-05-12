# Ask Implementation Notes

`axon ask` is a synchronous RAG command. It embeds the question with TEI, searches Qdrant, assembles context, and calls Gemini headless synthesis.

See [`docs/commands/ask.md`](../commands/ask.md) for CLI usage.
