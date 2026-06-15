package com.axon.app.ui.ask

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ChatBubbleTextTest {
    @Test
    fun `displayUserText decodes percent escaped prompts for readable bubbles`() {
        assertEquals("what is axon", displayUserText("what%20is%20axon"))
    }

    @Test
    fun `stripCitationText removes backend citation validation sections`() {
        val text = """
            Axon is a self-hosted RAG app. [S1]

            ## Citation Validation Failed
            - Answer contained no source citations.

            ## Retrieved Sources
            - https://github.com/jmagar/axon
            - https://docs.example.com/axon
        """.trimIndent()

        val stripped = stripCitationText(text)

        assertEquals("Axon is a self-hosted RAG app.", stripped)
        assertFalse(stripped.contains("Citation Validation Failed"))
        assertFalse(stripped.contains("Retrieved Sources"))
        assertFalse(stripped.contains("https://"))
    }

    @Test
    fun `stripSourcesBlock keeps inline markers but drops the sources block`() {
        val text = """
            Axon answers from indexed docs. [S1] [S2]

            ## Retrieved Sources
            - [S1] https://a.com/x
            - [S2] https://b.com/y
        """.trimIndent()

        val stripped = stripSourcesBlock(text)

        assertEquals("Axon answers from indexed docs. [S1] [S2]", stripped)
        assertFalse(stripped.contains("Retrieved Sources"))
        assertFalse(stripped.contains("https://"))
    }

    @Test
    fun `buildCitationLinks renumbers non-contiguous backend markers to 1-based display indices`() {
        val sources = listOf(
            AnswerSource(num = 3, url = "https://a.com/x", title = "x"),
            AnswerSource(num = 15, url = "https://b.com/y", title = "y"),
            AnswerSource(num = 2, url = "https://c.com/z", title = "z"),
        )

        val links = buildCitationLinks(sources)

        // Keyed by backend num; display index follows block order (1-based).
        assertEquals(3, links.size)
        assertEquals(1, links[3]?.displayIndex)
        assertEquals("https://a.com/x", links[3]?.url)
        assertEquals(2, links[15]?.displayIndex)
        assertEquals("https://b.com/y", links[15]?.url)
        assertEquals(3, links[2]?.displayIndex)
        assertEquals("https://c.com/z", links[2]?.url)
    }

    @Test
    fun `buildCitationLinks excludes sources without a backend marker`() {
        val sources = listOf(
            AnswerSource(num = 1, url = "https://a.com/x", title = "x"),
            AnswerSource(num = null, url = "https://b.com/y", title = "y"),
        )

        val links = buildCitationLinks(sources)

        // The unnumbered source is absent from the link map (but still parsed/listed).
        assertEquals(1, links.size)
        assertEquals(1, links[1]?.displayIndex)
        assertEquals("https://a.com/x", links[1]?.url)
        assertFalse(links.values.any { it.url == "https://b.com/y" })
    }

    @Test
    fun `buildCitationLinks collapses duplicate urls but the dedup happens upstream in parse`() {
        // parseAnswerSources dedupes by url, keeping the first backend number.
        val text = """
            Answer. [S1]

            ## Sources
            - [S1] https://a.com/x
            - [S2] https://a.com/x
        """.trimIndent()

        val sources = parseAnswerSources(text)
        assertEquals(1, sources.size)
        assertEquals(1, sources.first().num)

        val links = buildCitationLinks(sources)
        assertEquals(1, links.size)
        assertEquals(1, links[1]?.displayIndex)
    }

    @Test
    fun `parseAnswerSources truncates to 12 and 13th marker gets no link entry`() {
        val lines = (1..13).joinToString("\n") { "- [S$it] https://example.com/doc$it" }
        val text = "Answer.\n\n## Sources\n$lines"

        val sources = parseAnswerSources(text)
        assertEquals(12, sources.size)

        val links = buildCitationLinks(sources)
        assertEquals(12, links.size)
        // The 13th source's backend number is dropped by take(12).
        assertNull(links[13])
    }

    @Test
    fun `parseAnswerSources handles mixed numbered and unnumbered lines and strips trailing punctuation`() {
        val text = """
            Answer body.

            ## Sources
            - [S1] https://a.com/x
            - https://b.com/y
            - [S2] https://c.com/z.
        """.trimIndent()

        val sources = parseAnswerSources(text)

        assertEquals(3, sources.size)
        assertEquals(listOf(1, null, 2), sources.map { it.num })
        assertEquals("https://a.com/x", sources[0].url)
        assertEquals("https://b.com/y", sources[1].url)
        // Trailing period stripped from the 3rd URL.
        assertEquals("https://c.com/z", sources[2].url)
    }

    @Test
    fun `parseAnswerSources recognizes all header variants`() {
        val body = "\n- https://a.com/x"
        for (header in listOf("## Sources", "## Retrieved Sources", "## Citation Validation Failed")) {
            val text = "Answer.\n\n$header$body"
            val sources = parseAnswerSources(text)
            assertEquals("header `$header` should parse", 1, sources.size)
            assertEquals("https://a.com/x", sources.first().url)
        }
    }

    @Test
    fun `parseAnswerSources returns empty when there is no sources header`() {
        val text = "Just an answer with a bare https://a.com/x link and no header."
        assertTrue(parseAnswerSources(text).isEmpty())
    }

    @Test
    fun `sourceDisplayTitle prefers last path segment then host stem`() {
        assertEquals("scrape.md", sourceDisplayTitle("https://docs.example.com/axon/scrape.md"))
        // No path segment → host stem fallback.
        assertEquals("example", sourceDisplayTitle("https://example.com/"))
        assertEquals("github", sourceDisplayTitle("https://www.github.com"))
    }

    @Test
    fun `sourceDisplayTitle does not throw on malformed or empty input`() {
        // The contract is "never throws, always returns a non-null String fallback".
        for (input in listOf("", "://", "not a url at all %%%", "ftp://", "   ")) {
            val result = runCatching { sourceDisplayTitle(input) }
            assertTrue("must not throw on input '$input'", result.isSuccess)
            assertTrue("must return non-null for input '$input'", result.getOrNull() != null)
        }
    }
}
