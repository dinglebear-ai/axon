package com.axon.app.ui.jobs

import org.junit.Assert.assertFalse
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import com.axon.app.data.repository.JobUi
import kotlinx.serialization.json.Json

class JobsFormattersTest {
    @Test
    fun `active status helper includes only work-in-flight states`() {
        assertTrue(isActiveJobStatus("pending"))
        assertTrue(isActiveJobStatus("queued"))
        assertTrue(isActiveJobStatus("running"))
        assertTrue(isActiveJobStatus("processing"))
        assertTrue(isActiveJobStatus("in_progress"))

        assertFalse(isActiveJobStatus("completed"))
        assertFalse(isActiveJobStatus("done"))
        assertFalse(isActiveJobStatus("failed"))
        assertFalse(isActiveJobStatus("canceled"))
        assertFalse(isActiveJobStatus("idle"))
    }

    @Test
    fun `detail progress shows for active and successful terminal states only`() {
        assertTrue(shouldShowJobDetailProgress("running"))
        assertTrue(shouldShowJobDetailProgress("completed"))
        assertTrue(shouldShowJobDetailProgress("succeeded"))

        assertFalse(shouldShowJobDetailProgress("failed"))
        assertFalse(shouldShowJobDetailProgress("canceled"))
        assertFalse(shouldShowJobDetailProgress("idle"))
    }

    @Test
    fun `completed detail progress is always full`() {
        val job = JobUi(
            id = "job-1",
            status = "completed",
            url = "https://example.com",
            sourceType = null,
            target = null,
            errorText = null,
            resultJson = null,
        )

        assertEquals(1f, progressForJobDetail(job), 0.0f)
    }

    @Test
    fun `crawled page urls parse from inline result arrays`() {
        val result = Json.parseToJsonElement(
            """
            {
              "pages_crawled": 2,
              "crawled_urls": [
                "https://example.com/",
                {"url": "https://example.com/docs"}
              ],
              "events": [
                {"url": "https://example.com/error"}
              ]
            }
            """.trimIndent()
        )

        assertEquals(
            listOf("https://example.com/", "https://example.com/docs", "https://example.com/error"),
            crawledPageUrlsFromResult(result),
        )
    }

    @Test
    fun `crawl manifest path is inferred from output dir`() {
        val result = Json.parseToJsonElement(
            """
            {
              "output_dir": "/home/axon/.axon/output/domains/example.com/job-1",
              "pages_crawled": 2
            }
            """.trimIndent()
        )

        assertEquals(
            "domains/example.com/job-1/manifest.jsonl",
            crawlManifestArtifactPath(result),
        )
    }

    @Test
    fun `crawl manifest urls parse from jsonl`() {
        val manifest = """
            {"url":"https://example.com/","relative_path":"index.md","markdown_chars":100}
            {"url":"https://example.com/docs","relative_path":"docs.md","markdown_chars":100}
            nope
            {"url":"mailto:test@example.com","relative_path":"bad.md","markdown_chars":100}
        """.trimIndent()

        assertEquals(
            listOf("https://example.com/", "https://example.com/docs"),
            parseCrawlManifestUrls(manifest),
        )
    }
}
