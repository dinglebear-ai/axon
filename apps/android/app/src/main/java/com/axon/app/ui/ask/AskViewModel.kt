package com.axon.app.ui.ask

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.repository.AskResultUi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

sealed interface AskUiState {
    object Idle : AskUiState
    object Loading : AskUiState
    /**
     * [historyWarning] is non-null when the ask succeeded but saving to history
     * failed (e.g. disk full). The answer is still shown; the user is informed
     * that history was not recorded so they can act on it.
     */
    data class Success(val result: AskResultUi, val historyWarning: String? = null) : AskUiState
    data class Error(val message: String) : AskUiState
}

class AskViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _uiState = MutableStateFlow<AskUiState>(AskUiState.Idle)
    val uiState: StateFlow<AskUiState> = _uiState.asStateFlow()

    val history = container.axonRepository.recentHistory()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    fun ask(query: String) {
        if (query.isBlank()) return
        viewModelScope.launch {
            _uiState.value = AskUiState.Loading
            container.axonRepository.ask(query).fold(
                onSuccess = { result ->
                    val saved = container.axonRepository.recordAskHistory(
                        AskHistoryEntry(query = result.query, answer = result.answer)
                    )
                    _uiState.value = AskUiState.Success(
                        result = result,
                        historyWarning = if (!saved) {
                            "Answer shown, but history could not be saved (storage may be full)."
                        } else {
                            null
                        },
                    )
                },
                onFailure = { err ->
                    _uiState.value = AskUiState.Error(err.message ?: "Unknown error")
                },
            )
        }
    }
}
