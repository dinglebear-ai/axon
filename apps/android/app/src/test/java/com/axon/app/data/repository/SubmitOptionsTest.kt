package com.axon.app.data.repository

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class SubmitOptionsTest {
    @Test
    fun `site source options apply explicit request fields`() {
        val req =
            SiteSourceSubmitOptions(
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
    fun `empty site source options defer to server defaults`() {
        val req = SiteSourceSubmitOptions().requestFor("https://example.com")

        assertEquals(listOf("https://example.com"), req.urls)
        assertEquals(null, req.maxPages)
        assertEquals(null, req.maxDepth)
        assertEquals(null, req.renderMode)
        assertEquals(null, req.includeSubdomains)
    }

    @Test
    fun `source options apply canonical source fields`() {
        val req =
            SourceSubmitOptions(embed = false, collection = "docs")
                .requestFor(target = "github/octocat/Hello-World")

        assertEquals("github/octocat/Hello-World", req.source)
        assertEquals("docs", req.collection)
        assertFalse(req.embed!!)
    }

    @Test
    fun `empty source options defer to server defaults`() {
        val req =
            SourceSubmitOptions()
                .requestFor(target = "github/octocat/Hello-World")

        assertEquals("github/octocat/Hello-World", req.source)
        assertEquals(null, req.embed)
        assertEquals(null, req.collection)
    }
}
