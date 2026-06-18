package com.axon.app.ui.sessions

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.local.Session
import com.axon.app.data.remote.models.MobileSessionDto
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

class SessionsViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container
    private val database = container.database
    private val dao = database.sessionDao()
    private val askHistoryDao = database.askHistoryDao()
    private val repository = container.axonRepository

    private val _sessions = MutableStateFlow<List<Session>>(emptyList())
    val sessions: StateFlow<List<Session>> = _sessions.asStateFlow()

    val recentAsks: StateFlow<List<AskHistoryEntry>> = askHistoryDao.recent()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    init {
        refresh()
    }

    fun refresh() {
        viewModelScope.launch {
            val remote = repository.listMobileSessions().getOrNull()
            if (remote != null) {
                _sessions.value = remote.map { it.toLocalSession() }
            } else {
                _sessions.value = dao.allSessions().first()
            }
        }
    }

    fun pin(sessionId: String) {
        viewModelScope.launch {
            val ts = System.currentTimeMillis()
            repository.getMobileSession(sessionId)
                .map { it.copy(pinnedAt = ts, updatedAt = ts) }
                .mapCatching { repository.upsertMobileSession(it).getOrThrow() }
                .onSuccess { refresh() }
            dao.pin(sessionId, ts)
        }
    }

    fun unpin(sessionId: String) {
        viewModelScope.launch {
            repository.getMobileSession(sessionId)
                .map { it.copy(pinnedAt = null, updatedAt = System.currentTimeMillis()) }
                .mapCatching { repository.upsertMobileSession(it).getOrThrow() }
                .onSuccess { refresh() }
            dao.unpin(sessionId)
        }
    }

    fun delete(session: Session) {
        viewModelScope.launch {
            repository.deleteMobileSession(session.id)
            dao.delete(session)
            refresh()
        }
    }
}

private fun MobileSessionDto.toLocalSession(): Session =
    Session(
        id = id,
        title = title,
        firstMessagePreview = firstMessagePreview,
        turnCount = turnCount,
        injectedOpCount = injectedOpCount,
        createdAt = createdAt,
        updatedAt = updatedAt,
        pinnedAt = pinnedAt,
    )
