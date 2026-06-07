package com.axon.app.ui.sessions

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.local.Session
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

class SessionsViewModel(app: Application) : AndroidViewModel(app) {
    private val database = (app as AxonApp).container.database
    private val dao = database.sessionDao()
    private val askHistoryDao = database.askHistoryDao()

    val sessions: StateFlow<List<Session>> = dao.allSessions()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val recentAsks: StateFlow<List<AskHistoryEntry>> = askHistoryDao.recent()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    fun pin(sessionId: String) {
        viewModelScope.launch { dao.pin(sessionId, System.currentTimeMillis()) }
    }

    fun unpin(sessionId: String) {
        viewModelScope.launch { dao.unpin(sessionId) }
    }

    fun delete(session: Session) {
        viewModelScope.launch { dao.delete(session) }
    }
}
