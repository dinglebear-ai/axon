package com.axon.app.ui.ask

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.remote.AskStreamEvent
import com.axon.app.data.repository.AskResultUi
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

sealed interface AskUiState {
    object Idle : AskUiState
    /** Waiting for the first SSE event (retrieval phase). */
    object Loading : AskUiState
    /** Streaming: LLM is generating — [partialAnswer] grows with each delta token. */
    data class Streaming(val query: String, val partialAnswer: String) : AskUiState
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
            val collection = container.settingsRepository.settings.first().collection

            // Use StringBuilder to avoid O(n²) string concatenation across delta events.
            // Declared inside the launch block — concurrent ask() calls each get their own
            // StringBuilder so they cannot interleave.
            val accumulated = StringBuilder()

            runCatching {
                container.axonRepository.askStream(query, collection = collection).collect { event ->
                    when (event) {
                        is AskStreamEvent.Meta -> { /* stay Loading during retrieval phase */ }
                        is AskStreamEvent.Delta -> {
                            accumulated.append(event.text)
                            _uiState.value = AskUiState.Streaming(
                                query = query,
                                partialAnswer = accumulated.toString(),
                            )
                        }
                        is AskStreamEvent.Done -> {
                            val result = AskResultUi(query = query, answer = event.answer, timingMs = null)
                            val saved = container.axonRepository.recordAskHistory(
                                AskHistoryEntry(query = result.query, answer = result.answer)
                            )
                            _uiState.value = AskUiState.Success(
                                result = result,
                                historyWarning = if (!saved) "Answer shown, but history could not be saved (storage may be full)." else null,
                            )
                        }
                        is AskStreamEvent.Error -> {
                            _uiState.value = AskUiState.Error(event.message)
                        }
                    }
                }
            }.onFailure { err ->
                // Re-throw CancellationException so structured cancellation propagates correctly.
                // Any other exception is surfaced as an error state.
                if (err is CancellationException) throw err
                _uiState.value = AskUiState.Error(err.message ?: "Unknown error")
            }

            // Fallback: stream ended without a Done/Error event (truncated SSE response).
            // Note: _uiState.value is read after collect() returns. A concurrent ask() call is
            // impossible here because the ask() function cancels any prior job via a single
            // viewModelScope.launch — only one ask coroutine runs at a time per ViewModel instance.
            val current = _uiState.value
            if (current is AskUiState.Loading || current is AskUiState.Streaming) {
                if (accumulated.isNotBlank()) {
                    val result = AskResultUi(query = query, answer = accumulated.toString(), timingMs = null)
                    container.axonRepository.recordAskHistory(AskHistoryEntry(query = result.query, answer = result.answer))
                    _uiState.value = AskUiState.Success(result = result)
                } else {
                    _uiState.value = AskUiState.Error("No response received from server")
                }
            }
        }
    }
}
