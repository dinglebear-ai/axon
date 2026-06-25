package com.axon.app.data.repository

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class SubmitOptionsTest {
    @Test
    fun `crawl options apply explicit request fields`() {
        val req = CrawlSubmitOptions(
            maxPages = 1,
            maxDepth = 0,
            renderMode = "http",
            includeSubdomains = false,
        ).requestFor("https://example.com")

        assertEquals(listOf("https://example.com"), req.urls)
        assertEquals(1, req.maxPages)
        assertEquals(0, req.maxDepth)
        assertEquals("http", req.renderMode)
        assertFalse(req.includeSubdomains!!)
    }

    @Test
    fun `empty crawl options defer to server defaults`() {
        val req = CrawlSubmitOptions().requestFor("https://example.com")

        assertEquals(listOf("https://example.com"), req.urls)
        assertEquals(null, req.maxPages)
        assertEquals(null, req.maxDepth)
        assertEquals(null, req.renderMode)
        assertEquals(null, req.includeSubdomains)
    }

    @Test
    fun `ingest options apply metadata-only source exclusion`() {
        val req = IngestSubmitOptions(includeSource = false)
            .requestFor(sourceType = "github", target = "github/octocat/Hello-World")

        assertEquals("github", req.sourceType)
        assertEquals("github/octocat/Hello-World", req.target)
        assertFalse(req.includeSource!!)
    }

    @Test
    fun `empty ingest options defer to server defaults`() {
        val req = IngestSubmitOptions()
            .requestFor(sourceType = "github", target = "github/octocat/Hello-World")

        assertEquals("github", req.sourceType)
        assertEquals("github/octocat/Hello-World", req.target)
        assertEquals(null, req.includeSource)
    }
}
