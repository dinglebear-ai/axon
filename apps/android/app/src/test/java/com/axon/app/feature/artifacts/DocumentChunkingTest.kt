package com.axon.app.feature.artifacts

import com.axon.app.ui.common.DOC_CHUNK_TARGET_CHARS
import com.axon.app.ui.common.chunkDocument
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class DocumentChunkingTest {

    @Test
    fun `short content stays as a single chunk`() {
        val short = "hello world"
        val chunks = chunkDocument(short)
        assertEquals(1, chunks.size)
        assertEquals(short, chunks[0])
    }

    @Test
    fun `empty content returns a single empty chunk`() {
        val chunks = chunkDocument("")
        assertEquals(1, chunks.size)
        assertEquals("", chunks[0])
    }

    @Test
    fun `splits on paragraph boundaries when content exceeds target`() {
        // Two ~1500-char paragraphs joined by a blank line. Total >2000, so the
        // chunker must split — and the split must land on the `\n\n` boundary.
        // The separator is appended to the outgoing chunk so reassembly is lossless.
        val p1 = "a".repeat(1_500)
        val p2 = "b".repeat(1_500)
        val original = "$p1\n\n$p2"
        val chunks = chunkDocument(original)
        assertEquals(2, chunks.size)
        assertEquals(p1 + "\n\n", chunks[0])
        assertEquals(p2, chunks[1])
        assertEquals(original, chunks.joinToString(""))
    }

    @Test
    fun `oversized single paragraph falls back to line splitting`() {
        // A single paragraph (no \n\n) whose total > 2000 must still be split.
        // Lines are 800 chars each — three of them join into one paragraph >2000.
        val line = "x".repeat(800)
        val content = "$line\n$line\n$line"
        val chunks = chunkDocument(content)
        assertTrue("expected >1 chunk for line-fallback path, got ${chunks.size}", chunks.size > 1)
        // No chunk should ever exceed the target budget by more than one line's worth.
        chunks.forEach {
            assertTrue("chunk too large (${it.length}): $it", it.length <= DOC_CHUNK_TARGET_CHARS)
        }
        // Reassembling chunks must reproduce the original content.
        assertEquals(content, chunks.joinToString(""))
    }

    @Test
    fun `oversized single line is sliced by char as last resort`() {
        // No newlines at all, length > target — slice into 2KB blocks.
        val content = "z".repeat(5_000)
        val chunks = chunkDocument(content)
        assertEquals(3, chunks.size) // 2000 + 2000 + 1000
        assertEquals(2_000, chunks[0].length)
        assertEquals(2_000, chunks[1].length)
        assertEquals(1_000, chunks[2].length)
        // Reassembling the chunks must yield the original.
        assertEquals(content, chunks.joinToString(""))
    }

    @Test
    fun `markdown blocks sanitize schema anchors and object braces`() {
        val content = """
            # (resource) responses > (model) compacted_response > (schema)

            CompactedResponse object { id, created_at, object, 2 more } id: string ](<#(resource) responses > (model) compacted_response > (schema) > (property) id>) arguments: JSON string of the arguments.
            . detail: "low" or "high" or "auto" or "original"
            Inline skill details (</api/reference/resources/responses#(resource) responses > (model) inline_skill > (schema)>)
            ResponseOutputRefusal object { refusal, type } (<#(resource) responses > (model) r
        """.trimIndent()

        val text = markdownBlocks(content).joinToString(" ") { it.text }

        assertTrue(text.contains("object fields: id, created_at, object, 2 more"))
        assertTrue(text.contains("structured argument string"))
        assertTrue(text.none { it == '{' || it == '}' || it == '[' || it == ']' })
        assertTrue(!text.contains("](<#"))
        assertTrue(!text.contains("(resource)"))
        assertTrue(!text.contains("(schema)"))
        assertTrue(!text.contains("JSON string"))
        assertTrue(text.contains("detail:"))
        assertEquals(
            "detail: \"low\" or \"high\" or \"auto\" or \"original\"",
            markdownBlocks(""". detail: "low" or "high" or "auto" or "original"""").single().text,
        )
    }

    @Test
    fun `markdown code blocks drop stray wrapping backticks`() {
        val content = """
            ```
            `[ { x: 100, y: 200 } ]`
            ```
        """.trimIndent()

        val block = markdownBlocks(content).single()

        assertEquals(DocumentBlockKind.Code, block.kind)
        assertEquals("[ { x: 100, y: 200 } ]", block.text)
    }

    @Test
    fun `schema object paragraphs split into readable sections`() {
        val content = """
            CompactedResponse object { id, created_at } id: string The unique identifier. created_at: number Unix timestamp.
            ResponseOutputRefusal object { refusal, type } refusal: string The refusal explanation. type: string Always refusal.
        """.trimIndent()

        val blocks = markdownBlocks(content)

        assertEquals(DocumentBlockKind.Heading, blocks[0].kind)
        assertEquals("CompactedResponse", blocks[0].text)
        assertTrue(blocks[1].text.startsWith("object fields: id, created_at"))
        assertTrue(blocks[1].text.contains("\nid: string"))
        assertTrue(blocks[1].text.contains("\ncreated_at: number"))
        assertEquals(DocumentBlockKind.Heading, blocks[2].kind)
        assertEquals("ResponseOutputRefusal", blocks[2].text)
        assertTrue(blocks[3].text.contains("\nrefusal: string"))
        assertTrue(blocks[3].text.contains("\ntype: string"))
    }

    @Test
    fun `visible document warnings suppress redundant truncation copy`() {
        val warnings = listOf(
            "retrieve result truncated at 500 point(s)",
            "source had unsupported metadata",
        )

        assertEquals(
            listOf("source had unsupported metadata"),
            visibleDocumentWarnings(truncated = true, warnings = warnings),
        )
        assertEquals(warnings, visibleDocumentWarnings(truncated = false, warnings = warnings))
    }
}
