package com.axon.app.feature.ask

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class PromptAttachmentTest {
    @Test fun `full attachment quota performs no metadata or content reads`() =
        runTest {
            var metadataReads = 0
            var contentReads = 0
            val result =
                admitAttachmentCandidates(
                    candidates = (1..20).toList(),
                    existingNames = (1..MAX_ATTACHMENTS).map { "existing-$it" }.toSet(),
                    availableSlots = 0,
                    nameOf = {
                        metadataReads++
                        "file-$it"
                    },
                    read = {
                        contentReads++
                        Result.success(attachment("file-$it"))
                    },
                )
            assertEquals(0, metadataReads)
            assertEquals(0, contentReads)
            assertEquals(20, result.skipped)
        }

    @Test fun `oversized selection reads at most remaining unique slots`() =
        runTest {
            var contentReads = 0
            val result =
                admitAttachmentCandidates(
                    candidates = listOf("duplicate", "one", "two", "three", "four"),
                    existingNames = setOf("duplicate"),
                    availableSlots = 2,
                    nameOf = { it },
                    read = {
                        contentReads++
                        Result.success(attachment(it))
                    },
                )
            assertEquals(2, contentReads)
            assertEquals(listOf("one", "two"), result.accepted.map { it.name })
            assertEquals(3, result.skipped)
        }

    private fun attachment(name: String) = PromptAttachment(name, "text", false, 4)

    @Test
    fun `formatBytes renders zero as bytes`() {
        assertEquals("0 B", formatBytes(0L))
    }

    @Test
    fun `formatBytes renders sub-kilobyte values as raw bytes`() {
        assertEquals("1023 B", formatBytes(1023L))
    }

    @Test
    fun `formatBytes renders one kilobyte`() {
        assertEquals("%.1f KB".format(1.0), formatBytes(1024L))
    }

    @Test
    fun `formatBytes renders just-under-one-megabyte in kilobytes`() {
        assertEquals("%.1f KB".format(1048575L / 1024.0), formatBytes(1048575L))
    }

    @Test
    fun `formatBytes renders one megabyte`() {
        assertEquals("%.1f MB".format(1.0), formatBytes(1048576L))
    }

    @Test
    fun `formatBytes renders negative as empty string`() {
        assertEquals("", formatBytes(-1L))
    }
}
