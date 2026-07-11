package com.axon.app.feature.jobs

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

internal class JobsOverviewRefreshCoordinator(
    private val scope: CoroutineScope,
) {
    private val mutex = Mutex()
    private var inFlight: Deferred<Any?>? = null

    suspend fun <T> refresh(block: suspend () -> T): T {
        val deferred = mutex.withLock {
            @Suppress("UNCHECKED_CAST")
            inFlight?.takeIf { it.isActive } as Deferred<T>?
                ?: scope.async { block() }.also { next ->
                    inFlight = next as Deferred<Any?>
                    next.invokeOnCompletion {
                        if (inFlight === next) inFlight = null
                    }
                }
        }
        return deferred.await()
    }
}

internal class JobsOverviewPoller(
    private val scope: CoroutineScope,
    private val pollIntervalMs: Long,
    private val refresh: suspend () -> Unit,
) {
    private var pollJob: Job? = null

    fun setVisible(visible: Boolean) {
        if (visible) start() else stop()
    }

    private fun start() {
        if (pollJob?.isActive == true) return
        pollJob = scope.launch {
            while (true) {
                refresh()
                delay(pollIntervalMs)
            }
        }
    }

    private fun stop() {
        pollJob?.cancel()
        pollJob = null
    }
}
