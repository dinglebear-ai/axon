package com.axon.app.ui.sessions

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.local.Session
import com.axon.app.core.api.models.MobileSessionDto
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import java.util.Locale

private const val TAG = "SessionsViewModel"

class SessionsViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container
    private val database = container.database
    private val dao = database.sessionDao()
    private val askHistoryDao = database.askHistoryDao()
    private val repository = container.axonRepository

    private val _sessions = MutableStateFlow<List<Session>>(emptyList())
    val sessions: StateFlow<List<Session>> = _sessions.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    val recentAsks: StateFlow<List<AskHistoryEntry>> = askHistoryDao.recent()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    init {
        refresh()
    }

    fun refresh() {
        viewModelScope.launch {
            repository.listMobileSessions().fold(
                onSuccess = { remote ->
                    _error.value = null
                    val existingById = _sessions.value.associateBy { it.id }
                    _sessions.value = remote.map { it.toLocalSession(existingById[it.id]) }
                },
                onFailure = { cause ->
                    Log.w(TAG, "Failed to load mobile sessions", cause)
                    _error.value = sessionSyncMessage(cause)
                    _sessions.value = dao.allSessions().first()
                },
            )
        }
    }

    fun pin(sessionId: String) {
        viewModelScope.launch {
            val ts = System.currentTimeMillis()
            repository.getMobileSession(sessionId)
                .map { it.copy(pinnedAt = ts, updatedAt = ts) }
                .mapCatching { repository.upsertMobileSession(it).getOrThrow() }
                .fold(
                    onSuccess = {
                        dao.pin(sessionId, ts)
                        _error.value = null
                        refresh()
                    },
                    onFailure = { cause ->
                        Log.w(TAG, "Failed to pin mobile session $sessionId", cause)
                        _error.value = sessionSyncMessage(cause, "Could not pin synced session")
                    },
                )
        }
    }

    fun unpin(sessionId: String) {
        viewModelScope.launch {
            repository.getMobileSession(sessionId)
                .map { it.copy(pinnedAt = null, updatedAt = System.currentTimeMillis()) }
                .mapCatching { repository.upsertMobileSession(it).getOrThrow() }
                .fold(
                    onSuccess = {
                        dao.unpin(sessionId)
                        _error.value = null
                        refresh()
                    },
                    onFailure = { cause ->
                        Log.w(TAG, "Failed to unpin mobile session $sessionId", cause)
                        _error.value = sessionSyncMessage(cause, "Could not unpin synced session")
                    },
                )
        }
    }

    fun delete(session: Session) {
        viewModelScope.launch {
            repository.deleteMobileSession(session.id).fold(
                onSuccess = {
                    dao.delete(session)
                    _error.value = null
                    refresh()
                },
                onFailure = { cause ->
                    Log.w(TAG, "Failed to delete mobile session ${session.id}", cause)
                    _error.value = sessionSyncMessage(cause, "Could not delete synced session")
                },
            )
        }
    }
}

/**
 * Maps a server session onto the local Room cache. `status`/`sourceRefsJson`/
 * `draft`/`syncVersion` are not yet echoed by the server (see
 * `MobileSessionDto` kdoc), so when [existing] is supplied its values are
 * carried forward instead of being reset to the DTO defaults on every
 * refresh — otherwise a locally-set draft would be wiped by the next sync.
 */
private fun MobileSessionDto.toLocalSession(existing: Session? = null): Session =
    Session(
        id = id,
        title = title,
        firstMessagePreview = firstMessagePreview,
        turnCount = turnCount,
        injectedOpCount = injectedOpCount,
        createdAt = createdAt,
        updatedAt = updatedAt,
        pinnedAt = pinnedAt,
        status = existing?.status ?: status,
        sourceRefsJson = existing?.sourceRefsJson ?: Session.encodeSourceRefs(sourceRefs),
        draft = existing?.draft ?: draft,
        syncVersion = existing?.syncVersion ?: syncVersion,
    )

private fun sessionSyncMessage(cause: Throwable, fallback: String = "Could not sync sessions"): String {
    val message = cause.message.orEmpty()
    val lower = message.lowercase(Locale.US)
    return when {
        "<!doctype html" in lower || "<html" in lower || "expected start of the object" in lower ->
            "$fallback. The server returned a web page instead of the mobile sessions API."
        "401" in lower || "unauthorized" in lower ->
            "$fallback. Check your saved Axon auth in Settings."
        message.isBlank() -> fallback
        else -> message.lineSequence().firstOrNull()?.take(140) ?: fallback
    }
}
