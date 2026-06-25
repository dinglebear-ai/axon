package com.axon.app.ui.ask

import com.axon.app.ui.fab.FabOp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AskScreenHelpersTest {
    private fun attachment(name: String, content: String): PromptAttachment =
        PromptAttachment(name = name, content = content, truncated = false, sizeBytes = content.length.toLong())

    @Test
    fun `combinedAttachmentText returns null for no attachments`() {
        assertNull(combinedAttachmentText(emptyList()))
    }

    @Test
    fun `combinedAttachmentText wraps a single file in a name header`() {
        val text = combinedAttachmentText(listOf(attachment("notes.txt", "hello world")))
        assertEquals("=== notes.txt ===\nhello world", text)
    }

    @Test
    fun `combinedAttachmentText joins two files with a blank-line delimiter`() {
        val text = combinedAttachmentText(
            listOf(
                attachment("a.txt", "alpha"),
                attachment("b.txt", "beta"),
            ),
        )
        // The "=== name ===" header and the "\n\n" separator are the literal text
        // fed to the LLM, so this exact shape is load-bearing.
        assertEquals("=== a.txt ===\nalpha\n\n=== b.txt ===\nbeta", text)
    }

    @Test
    fun `chatSenderSide puts user messages on side 0`() {
        assertEquals(0, chatSenderSide(ChatItem.UserMsg("hi")))
    }

    @Test
    fun `chatSenderSide puts every non-user item on side 1`() {
        assertEquals(1, chatSenderSide(ChatItem.AxonMsg("answer")))
        assertEquals(1, chatSenderSide(ChatItem.Activity(name = "search", arg = "q", result = "running")))
        assertEquals(
            1,
            chatSenderSide(
                ChatItem.ActionResult(
                    op = FabOp.Scrape,
                    target = "https://example.com",
                    status = "200",
                    endpoint = "POST /v1/{operation}",
                    summary = "ok",
                    body = "{}",
                ),
            ),
        )
        assertEquals(
            1,
            chatSenderSide(ChatItem.Injection(op = FabOp.Scrape, target = "https://example.com")),
        )
    }

    @Test
    fun `compactSingleLine collapses whitespace and trims long hit text`() {
        val text = compactSingleLine(
            "A title\nwith   spacing and a very long tail that should be shortened",
            limit = 28,
        )

        assertEquals("A title with spacing and a...", text)
    }

    @Test
    fun `compactHitCountNote reports hidden results`() {
        assertEquals("", compactHitCountNote(COMPACT_HIT_LIMIT))
        assertEquals("\n\nShowing 2 of 5 results.", compactHitCountNote(5))
    }
}
