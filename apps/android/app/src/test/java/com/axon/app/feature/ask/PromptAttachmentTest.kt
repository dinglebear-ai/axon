package com.axon.app.feature.ask

import org.junit.Assert.assertEquals
import org.junit.Test

class PromptAttachmentTest {
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
