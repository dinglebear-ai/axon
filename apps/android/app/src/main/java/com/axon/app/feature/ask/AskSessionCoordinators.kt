package com.axon.app.feature.ask

import com.axon.app.core.api.models.MobileSessionDto
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

internal fun interface MobileSessionWriter {
    suspend fun upsert(session: MobileSessionDto): Result<MobileSessionDto>
}

internal class AskSessionPersistenceCoordinator(
    private val writer: MobileSessionWriter,
) {
    private val mutex = Mutex()
    private var generation = 0L
    private var metadata: MobileSessionDto? = null
    private val _saveError = MutableStateFlow<String?>(null)
    val saveError: StateFlow<String?> = _saveError.asStateFlow()

    fun select(session: MobileSessionDto?) {
        generation++
        metadata = session
        _saveError.value = null
    }

    suspend fun save(
        sessionId: String,
        createdAt: Long,
        updatedAt: Long,
        items: List<ChatItem>,
    ): Result<MobileSessionDto> {
        val saveGeneration = generation
        return mutex.withLock {
            val prior = metadata?.takeIf { it.id == sessionId && generation == saveGeneration }
            writer.upsert(buildMobileSessionDto(sessionId, createdAt, updatedAt, items, prior))
                .onSuccess {
                    if (generation == saveGeneration && it.id == sessionId) {
                        metadata = it
                        _saveError.value = null
                    }
                }.onFailure { cause ->
                    if (generation == saveGeneration) {
                        _saveError.value =
                            cause.message
                                ?: "Could not save this chat session. Check your connection and sign in again."
                    }
                }
        }
    }
}

internal class AskSessionLoadCoordinator(
    private val scope: CoroutineScope,
    private val loadSession: suspend (String) -> Result<MobileSessionDto>,
    private val applySession: (MobileSessionDto) -> Unit,
    private val reportFailure: (String, Throwable) -> Unit,
) {
    private var generation = 0L
    private var job: Job? = null

    fun invalidate() {
        generation++
        job?.cancel()
        job = null
    }

    fun load(sessionId: String) {
        val requestGeneration = ++generation
        job?.cancel()
        job =
            scope.launch {
                loadSession(sessionId).fold(
                    onSuccess = { if (requestGeneration == generation) applySession(it) },
                    onFailure = { if (requestGeneration == generation) reportFailure(sessionId, it) },
                )
            }
    }
}

internal class AskRegenerationState {
    var producedTurn = false
        private set

    fun started() {
        producedTurn = false
    }

    fun completed() {
        producedTurn = true
    }

    fun turnsBeforeRegeneration(turns: List<AskTurn>) = if (producedTurn) turns.dropLast(1) else turns
}
