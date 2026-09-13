package com.axon.app.ui.summarize

import com.axon.app.data.repository.SummarizeResultUi
import com.axon.app.ui.common.Resource
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SummarizeViewModelTest {
    @Test fun `production coordinator publishes successful summary`() =
        runTest {
            val expected = SummarizeResultUi(listOf("https://a"), "ok", 7, false)
            val coordinator = SummarizeCoordinator(this, { "docs" }) { _, _ -> Result.success(expected) }
            coordinator.submit("https://a")
            testScheduler.advanceUntilIdle()
            assertEquals(Resource.Ready(expected), coordinator.uiState.value)
        }

    @Test fun `invalid URL never calls repository`() =
        runTest {
            var calls = 0
            val coordinator =
                SummarizeCoordinator(this, { null }) { _, _ ->
                    calls++
                    error("must not run")
                }
            coordinator.submit("not-a-url")
            testScheduler.advanceUntilIdle()
            assertEquals(Resource.Idle, coordinator.uiState.value)
            assertEquals(0, calls)
        }

    @Test fun `new submission cancels stale work and only latest result is published`() =
        runTest {
            val first = CompletableDeferred<Result<SummarizeResultUi>>()
            val second = CompletableDeferred<Result<SummarizeResultUi>>()
            val coordinator =
                SummarizeCoordinator(this, { null }) { urls, _ ->
                    if (urls.single().endsWith("first")) first.await() else second.await()
                }
            coordinator.submit("https://example.com/first")
            testScheduler.runCurrent()
            coordinator.submit("https://example.com/second")
            testScheduler.runCurrent()
            second.complete(Result.success(SummarizeResultUi(listOf("second"), "latest", 1, false)))
            testScheduler.runCurrent()
            first.complete(Result.success(SummarizeResultUi(listOf("first"), "stale", 1, false)))
            testScheduler.advanceUntilIdle()
            assertEquals("latest", (coordinator.uiState.value as Resource.Ready<SummarizeResultUi>).value.summary)
        }

    @Test fun `failure is published by production coordinator`() =
        runTest {
            val coordinator = SummarizeCoordinator(this, { null }) { _, _ -> Result.failure(IllegalStateException("boom")) }
            coordinator.submit("https://example.com")
            testScheduler.advanceUntilIdle()
            assertTrue((coordinator.uiState.value as Resource.Error).message.contains("boom"))
        }
}
