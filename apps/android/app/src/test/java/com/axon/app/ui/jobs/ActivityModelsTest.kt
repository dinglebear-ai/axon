package com.axon.app.ui.jobs

import com.axon.app.data.repository.JobFamily
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.RecentJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ActivityModelsTest {
    @Test fun `recent activity rows prefer live job state when available`() {
        val recent = listOf(RecentJob(jobId = "job-1", kind = "crawl", target = "https://example.test", submittedAt = 100L))
        val live = JobUi(
            kind = JobFamily.Crawl,
            id = "job-1",
            status = "failed",
            url = "https://example.test/live",
            sourceType = null,
            target = null,
            errorText = "boom",
            resultJson = null,
        )

        val rows = recentActivityRows(recent, mapOf(JobFamily.Crawl to listOf(live)))

        assertEquals(1, rows.size)
        assertTrue(rows.single().live)
        assertEquals("failed", rows.single().job.status)
        assertEquals("boom", rows.single().job.errorText)
        assertEquals("https://example.test/live", rows.single().job.url)
    }

    @Test fun `recent activity rows keep submitted fallback when server job is unavailable`() {
        val recent = listOf(RecentJob(jobId = "job-2", kind = "ingest", target = "github.com/o/r", submittedAt = 200L))

        val rows = recentActivityRows(recent, emptyMap())

        assertEquals(1, rows.size)
        assertFalse(rows.single().live)
        assertEquals(JobFamily.Ingest, rows.single().kind)
        assertEquals("submitted", rows.single().job.status)
        assertEquals("github.com/o/r", rows.single().job.target)
    }

    @Test fun `recent activity ignores unknown future job kinds`() {
        val recent = listOf(RecentJob(jobId = "job-3", kind = "future", target = "x", submittedAt = 300L))

        assertEquals(emptyList<ActivityJobRow>(), recentActivityRows(recent, emptyMap()))
    }
}
