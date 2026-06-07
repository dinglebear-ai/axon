package com.axon.app.ui.ask

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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
    fun `extractedCitationLabels converts inline markers and retrieved urls`() {
        val text = """
            Axon can answer from indexed docs. [S2]

            ## Retrieved Sources
            - https://github.com/jmagar/axon
            - https://docs.example.com/axon/guide
        """.trimIndent()

        val labels = extractedCitationLabels(text)

        assertTrue(labels.contains("S2"))
        assertTrue(labels.contains("github"))
        assertTrue(labels.contains("docs.example"))
    }
}
