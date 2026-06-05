package com.axon.app.ui.jobs

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.delay
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class JobsOverviewRefreshCoordinatorTest {

    @Test fun `overlapping refresh callers share one in-flight load`() = runTest {
        val coordinator = JobsOverviewRefreshCoordinator(backgroundScope)
        var calls = 0

        val first = async {
            coordinator.refresh {
                calls++
                delay(100)
                1
            }
        }
        val second = async {
            coordinator.refresh {
                calls++
                delay(100)
                2
            }
        }

        val results = awaitAll(first, second)

        assertEquals(listOf(1, 1), results)
        assertEquals(1, calls)
    }

    @Test fun `refresh after prior completion starts a new load`() = runTest {
        val coordinator = JobsOverviewRefreshCoordinator(backgroundScope)
        var calls = 0

        assertEquals(1, coordinator.refresh { ++calls })
        assertEquals(2, coordinator.refresh { ++calls })

        assertEquals(2, calls)
    }
}
