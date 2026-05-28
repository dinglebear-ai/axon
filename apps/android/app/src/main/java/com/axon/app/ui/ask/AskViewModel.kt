package com.axon.app.ui.ask

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.remote.AskStreamEvent
import com.axon.app.data.repository.AskResultUi
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** A single completed Q/A turn kept in-VM for follow-up context injection. */
data class AskTurn(val question: String, val answer: String)

/** Maximum prior turns inlined into the next ask. Matches CLI's MAX_FOLLOW_UP_TURNS=6. */
internal const val MAX_FOLLOW_UP_TURNS = 6

/**
 * Build the effective query for the server by prepending the last
 * [MAX_FOLLOW_UP_TURNS] turns as "Q: …\nA: …" pairs.
 *
 * Mirrors the CLI's render in `src/cli/commands/ask/followup.rs::follow_up_query`.
 */
internal fun buildFollowUpQuery(prior: List<AskTurn>, question: String): String {
    if (prior.isEmpty()) return question
    val recent = prior.takeLast(MAX_FOLLOW_UP_TURNS)
    val rendered = recent.joinToString("\n\n") { "Q: ${it.question}\nA: ${it.answer}" }
    return "$rendered\n\n$question"
}

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

    private val _turns = MutableStateFlow<List<AskTurn>>(emptyList())
    val turns: StateFlow<List<AskTurn>> = _turns.asStateFlow()

    /**
     * In-flight ask coroutine. Tracked so a second `ask()` call cancels the
     * prior stream — without this, repeated Asks pile up parallel SSE
     * connections, blocked OkHttp IO threads (readLine never returns until
     * STREAM_READ_TIMEOUT_SECONDS = 300s), and interleaved [_uiState] writes.
     * The user-visible symptom is an app that "hangs" and then force-closes.
     */
    private var askJob: Job? = null

    /** Drops all in-VM turns. Called by OperationsScreen on mode-switch away from Ask. */
    fun clearFollowUp() { _turns.value = emptyList() }

    private fun appendTurn(q: String, a: String) {
        _turns.value = (_turns.value + AskTurn(q, a)).takeLast(MAX_FOLLOW_UP_TURNS)
    }

    fun ask(query: String) {
        if (query.isBlank()) return
        // Cancel any prior in-flight stream BEFORE launching a new one. Without
        // this guard, viewModelScope.launch creates parallel coroutines and a
        // stuck readLine() from a previous ask leaks an IO thread.
        askJob?.cancel()
        askJob = viewModelScope.launch {
            _uiState.value = AskUiState.Loading
            val collection = container.settingsRepository.settings.first().collection

            // Prepend prior turns into the wire query, but keep the raw `query` for UI/history
            // so we don't nest prior context inside future turns.
            val effective = buildFollowUpQuery(_turns.value, query)

            // Use StringBuilder to avoid O(n²) string concatenation across delta events.
            // Declared inside the launch block — concurrent ask() calls each get their own
            // StringBuilder so they cannot interleave.
            val accumulated = StringBuilder()

            runCatching {
                container.axonRepository.askStream(effective, collection = collection).collect { event ->
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
                            appendTurn(q = query, a = event.answer)
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
            // [askJob] tracking guarantees at most one ask coroutine writes to _uiState
            // at a time — see the askJob?.cancel() above.
            val current = _uiState.value
            if (current is AskUiState.Loading || current is AskUiState.Streaming) {
                if (accumulated.isNotBlank()) {
                    val finalAnswer = accumulated.toString()
                    val result = AskResultUi(query = query, answer = finalAnswer, timingMs = null)
                    val saved = container.axonRepository.recordAskHistory(
                        AskHistoryEntry(query = result.query, answer = result.answer),
                    )
                    // Honest about the truncation — the user is shown the partial bytes
                    // but warned that the stream ended before the server signalled Done.
                    val warning = buildString {
                        append("Response may be incomplete — the server ended the stream without a completion event.")
                        if (!saved) append(" History could not be saved (storage may be full).")
                    }
                    _uiState.value = AskUiState.Success(result = result, historyWarning = warning)
                    appendTurn(q = query, a = finalAnswer)
                } else {
                    _uiState.value = AskUiState.Error("No response received from server")
                }
            }
        }
    }
}
