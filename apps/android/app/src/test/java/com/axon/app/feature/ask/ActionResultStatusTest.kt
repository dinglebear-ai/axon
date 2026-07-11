package com.axon.app.feature.ask

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ActionResultStatusTest {
    @Test
    fun `status classifier separates success queued and error states`() {
        assertEquals(ResultStatusKind.Success, resultStatusKind("200 OK"))
        assertEquals(ResultStatusKind.Warning, resultStatusKind("202 Accepted"))
        assertEquals(ResultStatusKind.Warning, resultStatusKind("running"))
        assertEquals(ResultStatusKind.Error, resultStatusKind("500 Internal Server Error"))
        assertEquals(ResultStatusKind.Error, resultStatusKind("failed"))
    }

    @Test
    fun `injection status classifier keeps accepted jobs pending`() {
        assertEquals(true, isFinalSuccessfulStatus("200 OK"))
        assertEquals(true, isFinalSuccessfulStatus("complete"))
        assertEquals(false, isFinalSuccessfulStatus("202 Accepted"))
        assertEquals(false, isFinalSuccessfulStatus("queued"))
        assertEquals(false, isFinalSuccessfulStatus("running"))
        assertEquals(false, isFinalSuccessfulStatus("failed"))
    }

    @Test
    fun `action result body preview clamps long search output`() {
        val rendered = compactActionResultBody(
            (1..12).joinToString("\n") { index ->
                "Result $index https://example.com/${"long-path-segment".repeat(8)}"
            },
            maxLines = 4,
            maxChars = 160,
        )

        assertTrue(rendered.lines().size <= 5)
        assertTrue(rendered.contains("Result 1"))
        assertTrue(!rendered.contains("Result 12"))
        assertTrue(rendered.endsWith("...truncated in chat"))
        assertTrue(rendered.length <= 185)
    }
}
