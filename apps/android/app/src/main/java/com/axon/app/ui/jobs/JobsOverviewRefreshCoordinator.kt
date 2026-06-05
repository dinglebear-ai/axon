package com.axon.app.ui.jobs

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.async
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
