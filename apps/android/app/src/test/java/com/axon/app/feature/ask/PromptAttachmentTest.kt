package com.axon.app.feature.ask

import kotlinx.coroutines.CancellationException
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
                        Result.success("file-$it")
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
                    nameOf = { Result.success(it) },
                    read = {
                        contentReads++
                        Result.success(attachment(it))
                    },
                )
            assertEquals(2, contentReads)
            assertEquals(listOf("one", "two"), result.accepted.map { it.name })
            assertEquals(3, result.skipped)
        }

    @Test fun `metadata failure skips only that candidate and continues admission`() =
        runTest {
            val result =
                admitAttachmentCandidates(
                    candidates = listOf("revoked", "good"),
                    existingNames = emptySet(),
                    availableSlots = 2,
                    nameOf = {
                        if (it == "revoked") Result.failure(SecurityException("permission revoked"))
                        else Result.success("good.txt")
                    },
                    read = { Result.success(attachment("good.txt")) },
                )

            assertEquals(listOf("good.txt"), result.accepted.map { it.name })
            assertEquals(1, result.failed.size)
            assertEquals("permission revoked", result.failed.single().message)
        }

    @Test fun `metadata cancellation stops admission before the next candidate`() =
        runTest {
            var metadataReads = 0
            var contentReads = 0

            try {
                admitAttachmentCandidates(
                    candidates = listOf("cancelled", "must-not-run"),
                    existingNames = emptySet(),
                    availableSlots = 2,
                    nameOf = {
                        metadataReads++
                        Result.failure(CancellationException("picker cancelled"))
                    },
                    read = {
                        contentReads++
                        Result.success(attachment(it))
                    },
                )
                throw AssertionError("expected cancellation")
            } catch (error: CancellationException) {
                assertEquals("picker cancelled", error.message)
            }

            assertEquals(1, metadataReads)
            assertEquals(0, contentReads)
        }

    @Test fun `content cancellation stops admission before the next candidate`() =
        runTest {
            var metadataReads = 0
            var contentReads = 0

            try {
                admitAttachmentCandidates(
                    candidates = listOf("cancelled", "must-not-run"),
                    existingNames = emptySet(),
                    availableSlots = 2,
                    nameOf = {
                        metadataReads++
                        Result.success("$it.txt")
                    },
                    read = {
                        contentReads++
                        Result.failure(CancellationException("read cancelled"))
                    },
                )
                throw AssertionError("expected cancellation")
            } catch (error: CancellationException) {
                assertEquals("read cancelled", error.message)
            }

            assertEquals(1, metadataReads)
            assertEquals(1, contentReads)
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
