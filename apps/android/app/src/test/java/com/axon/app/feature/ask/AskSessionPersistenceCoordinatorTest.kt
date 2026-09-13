package com.axon.app.feature.ask

import com.axon.app.core.api.models.MobileSessionDto
import com.axon.app.data.repository.AskResultUi
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AskSessionPersistenceCoordinatorTest {
    @Test
    fun `failed save remains observable while caller keeps completed answer state`() = runTest {
        val coordinator =
            AskSessionPersistenceCoordinator(
                MobileSessionWriter { Result.failure(IllegalStateException("sync version conflict")) },
            )
        val completedAnswer = AskUiState.Success(AskResultUi(query = "question", answer = "answer", timingMs = null))

        val result = coordinator.save("s", 1, 2, listOf(ChatItem.UserMsg("question")))

        assertTrue(result.isFailure)
        assertEquals("sync version conflict", coordinator.saveError.value)
        assertEquals("answer", completedAnswer.result.answer)
    }

    @Test
    fun `successful save and session selection clear a previous save error`() = runTest {
        var fail = true
        val coordinator =
            AskSessionPersistenceCoordinator(
                MobileSessionWriter { session ->
                    if (fail) Result.failure(IllegalStateException("conflict"))
                    else Result.success(session.copy(syncVersion = 2))
                },
            )
        val items = listOf(ChatItem.UserMsg("question"))
        coordinator.save("s", 1, 2, items)
        assertEquals("conflict", coordinator.saveError.value)

        fail = false
        coordinator.save("s", 1, 3, items)
        assertEquals(null, coordinator.saveError.value)

        fail = true
        coordinator.save("s", 1, 4, items)
        coordinator.select(null)
        assertEquals(null, coordinator.saveError.value)
    }

    @Test fun `create second save reload and third save echo versions and preserve metadata`() =
        runTest {
            val stored = mutableMapOf<String, MobileSessionDto>()
            val coordinator =
                AskSessionPersistenceCoordinator { submitted ->
                    val current = stored[submitted.id]
                    if (current != null && submitted.syncVersion != current.syncVersion) {
                        Result.failure(IllegalStateException("version conflict"))
                    } else {
                        val saved = submitted.copy(syncVersion = (current?.syncVersion ?: 0) + 1)
                        stored[saved.id] = saved
                        Result.success(saved)
                    }
                }
            val firstItems = listOf<ChatItem>(ChatItem.UserMsg("one"))
            coordinator.select(null)
            assertEquals(1L, coordinator.save("s", 1, 2, firstItems).getOrThrow().syncVersion)
            val twoItems = firstItems + ChatItem.AxonMsg("answer", false)
            assertEquals(2L, coordinator.save("s", 1, 3, twoItems).getOrThrow().syncVersion)

            val loaded = stored.getValue("s").copy(pinnedAt = 9, sourceRefs = listOf("source"), draft = "draft")
            coordinator.select(loaded)
            val saved = coordinator.save("s", 1, 4, twoItems + ChatItem.UserMsg("two")).getOrThrow()
            assertEquals(3L, saved.syncVersion)
            assertEquals(9L, saved.pinnedAt)
            assertEquals(listOf("source"), saved.sourceRefs)
            assertEquals("draft", saved.draft)
            assertEquals(3, saved.items.size)
        }

    @Test fun `overlapping saves serialize and stale session acknowledgement is not reused`() =
        runTest {
            val firstRelease = CompletableDeferred<Unit>()
            val submitted = mutableListOf<MobileSessionDto>()
            val coordinator =
                AskSessionPersistenceCoordinator { session ->
                    submitted += session
                    if (session.id == "a") firstRelease.await()
                    Result.success(session.copy(syncVersion = (session.syncVersion ?: 0) + 1))
                }
            coordinator.select(null)
            val first = async { coordinator.save("a", 1, 2, listOf(ChatItem.UserMsg("a"))) }
            testScheduler.runCurrent()
            coordinator.select(null)
            val second = async { coordinator.save("b", 3, 4, listOf(ChatItem.UserMsg("b"))) }
            testScheduler.runCurrent()
            assertEquals(listOf("a"), submitted.map { it.id })
            firstRelease.complete(Unit)
            first.await()
            second.await()
            assertEquals(listOf("a", "b"), submitted.map { it.id })
            assertEquals(null, submitted.last().syncVersion)
        }

    @Test fun `actual stale version conflict is surfaced`() =
        runTest {
            val coordinator =
                AskSessionPersistenceCoordinator {
                    Result.failure(IllegalStateException("version conflict"))
                }
            coordinator.select(session("s", 2))
            val result = coordinator.save("s", 1, 2, listOf(ChatItem.UserMsg("x")))
            assertTrue(result.isFailure)
            assertTrue(result.exceptionOrNull()?.message?.contains("conflict") == true)
        }

    @Test fun `older load cannot replace newer selection or new session`() =
        runTest {
            val pending = mutableMapOf<String, CompletableDeferred<Result<MobileSessionDto>>>()
            val applied = mutableListOf<String>()
            val failures = mutableListOf<String>()
            val coordinator =
                AskSessionLoadCoordinator(
                    scope = this,
                    loadSession = { pending.getValue(it).await() },
                    applySession = { applied += it.id },
                    reportFailure = { id, _ -> failures += id },
                )

            pending["a"] = CompletableDeferred()
            pending["b"] = CompletableDeferred()
            coordinator.load("a")
            testScheduler.runCurrent()
            coordinator.load("b")
            testScheduler.runCurrent()
            pending.getValue("b").complete(Result.success(session("b", 1)))
            testScheduler.runCurrent()
            pending.getValue("a").complete(Result.success(session("a", 1)))
            testScheduler.advanceUntilIdle()
            assertEquals(listOf("b"), applied)

            pending["c"] = CompletableDeferred()
            coordinator.load("c")
            testScheduler.runCurrent()
            coordinator.invalidate()
            pending.getValue("c").complete(Result.failure(IllegalStateException("stale")))
            testScheduler.advanceUntilIdle()
            assertEquals(listOf("b"), applied)
            assertTrue(failures.isEmpty())
        }

    private fun session(
        id: String,
        version: Long,
    ) = MobileSessionDto(
        id = id,
        title = id,
        firstMessagePreview = "",
        createdAt = 1,
        updatedAt = 1,
        syncVersion = version,
    )
}
