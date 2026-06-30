package com.axon.app.ui.ask

import android.os.SystemClock
import androidx.lifecycle.viewModelScope
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.remote.AskStreamEvent
import com.axon.app.data.repository.AskResultUi
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

internal fun AskViewModel.askFromQuery(query: String, attachment: String? = null) {
    if (query.isBlank()) return
    // Remember the attachment of the question being asked so regenerateLast()
    // can re-run the same input (attachment text is never stored in turns/history).
    lastAttachment = attachment
    lastAskProducedTurn = false
    val mode = _mode.value
    // Cancel any prior in-flight stream BEFORE launching a new one. Without
    // this guard, viewModelScope.launch creates parallel coroutines and a
    // stuck readLine() from a previous ask leaks an IO thread.
    askJob?.cancel()
    askJob = viewModelScope.launch {
        appendItem(ChatItem.UserMsg(query))
        appendItem(ChatItem.AxonMsg("", isStreaming = true))
        _uiState.value = AskUiState.Loading
        // An attached document is fed straight to the LLM via the chat path:
        // RAG retrieval would embed the whole file as the query and reject it
        // ("no candidates passed topical overlap"), so attachments bypass it.
        val useRag = mode == ConversationMode.Ask && attachment.isNullOrBlank()
        val collection = if (useRag) {
            container.settingsRepository.settings.first().collection
        } else {
            null
        }

        // Prepend prior turns into the wire query, but keep the raw `query` for UI/history
        // so we don't nest prior context inside future turns. An attachment's text is
        // inlined into the current question only — never stored in turns/history, so it
        // doesn't leak into later follow-ups.
        val questionWithAttachment = if (!attachment.isNullOrBlank()) {
            "Attached document:\n\"\"\"\n$attachment\n\"\"\"\n\nUsing the attached document above, answer:\n$query"
        } else {
            query
        }
        val effective = buildFollowUpQuery(_turns.value, questionWithAttachment)

        // Use StringBuilder to avoid O(n²) string concatenation across delta events.
        // Declared inside the launch block — concurrent ask() calls each get their own
        // StringBuilder so they cannot interleave.
        val accumulated = StringBuilder()
        var lastFlushMs = 0L

        fun flushStreaming(force: Boolean = false) {
            val now = SystemClock.elapsedRealtime()
            if (!force && now - lastFlushMs < STREAM_UI_FLUSH_MS) return
            val text = accumulated.toString()
            _uiState.value = AskUiState.Streaming(query = query, partialAnswer = text)
            replaceLastAxonMsg(text, isStreaming = true)
            lastFlushMs = now
        }

        runCatching {
            val stream = if (useRag) {
                container.axonRepository.askStream(effective, collection = collection)
            } else {
                container.axonRepository.chatStream(effective)
            }
            stream.collect { event ->
                when (event) {
                    is AskStreamEvent.Meta -> {
                        if (useRag) appendOrUpdateActivity(event.phase, query)
                    }
                    is AskStreamEvent.Delta -> {
                        accumulated.append(event.text)
                        flushStreaming()
                    }
                    is AskStreamEvent.Done -> {
                        if (accumulated.isNotBlank()) flushStreaming(force = true)
                        val finalAnswer = resolvedDoneAnswer(
                            doneAnswer = event.answer,
                            accumulatedAnswer = accumulated.toString(),
                        )
                        if (finalAnswer.isBlank()) {
                            _uiState.value = AskUiState.Error("No response received from server")
                            replaceLastAxonMsg(userFacingAskError("No response received from server"), isStreaming = false)
                            return@collect
                        }
                        val result = AskResultUi(query = query, answer = finalAnswer, timingMs = null)
                        val saved = container.axonRepository.recordAskHistory(
                            AskHistoryEntry(query = result.query, answer = result.answer),
                        )
                        _uiState.value = AskUiState.Success(
                            result = result,
                            historyWarning = if (!saved) "Answer shown, but history could not be saved (storage may be full)." else null,
                        )
                        completeActivities(persist = false)
                        replaceLastAxonMsg(finalAnswer, isStreaming = false)
                        appendTurn(q = query, a = finalAnswer)
                        lastAskProducedTurn = true
                    }
                    is AskStreamEvent.Error -> {
                        if (accumulated.isNotBlank()) flushStreaming(force = true)
                        _uiState.value = AskUiState.Error(event.message)
                        replaceLastAxonMsg(userFacingAskError(event.message), isStreaming = false)
                    }
                }
            }
        }.onFailure { err ->
            // Re-throw CancellationException so structured cancellation propagates correctly.
            // Any other exception is surfaced as an error state.
            if (err is CancellationException) throw err
            val message = err.message ?: "Unknown error"
            _uiState.value = AskUiState.Error(message)
            replaceLastAxonMsg(userFacingAskError(message), isStreaming = false)
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
                completeActivities(persist = false)
                replaceLastAxonMsg(finalAnswer, isStreaming = false)
                appendTurn(q = query, a = finalAnswer)
                lastAskProducedTurn = true
            } else {
                _uiState.value = AskUiState.Error("No response received from server")
                replaceLastAxonMsg(userFacingAskError("No response received from server"), isStreaming = false)
            }
        }
    }
}

internal fun userFacingAskError(message: String): String {
    val detail = message.trim().ifBlank { "Unknown error" }
    return when {
        detail.contains("No candidates passed topical overlap", ignoreCase = true) ->
            "Error: I couldn't find indexed context for this question. Switch to Chat for a general answer, or use + to Search, Crawl, Embed, or Retrieve the relevant source first.\n\nDetail: $detail"
        detail.contains("No response received from server", ignoreCase = true) ->
            "Error: Axon did not receive a response from the server. Check the connection status, then retry or switch to Chat if this is a general question.\n\nDetail: $detail"
        else -> "Error: Axon couldn't complete this request. Retry, switch modes, or use + to gather the source material first.\n\nDetail: $detail"
    }
}
