package com.axon.app.data.util

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SharedUrlExtractorTest {
    @Test
    fun `extracts plain shared https url`() {
        assertEquals(
            "https://example.com/docs",
            SharedUrlExtractor.firstHttpUrl("https://example.com/docs"),
        )
    }

    @Test
    fun `extracts first url from surrounding text`() {
        assertEquals(
            "https://example.com/a?x=1",
            SharedUrlExtractor.firstHttpUrl("Read this: https://example.com/a?x=1 and then https://example.org"),
        )
    }

    @Test
    fun `trims sentence punctuation around shared url`() {
        assertEquals(
            "https://example.com/path",
            SharedUrlExtractor.firstHttpUrl("Crawl (https://example.com/path)."),
        )
    }

    @Test
    fun `rejects non-http shared text`() {
        assertNull(SharedUrlExtractor.firstHttpUrl("file:///tmp/doc.md"))
        assertNull(SharedUrlExtractor.firstHttpUrl("example.com/docs"))
        assertNull(SharedUrlExtractor.firstHttpUrl("not a url"))
    }
}
