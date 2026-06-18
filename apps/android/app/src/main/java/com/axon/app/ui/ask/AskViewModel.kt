package com.axon.app.ui.ask

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.repository.JobFamily
import com.axon.app.data.repository.RecentJob
import com.axon.app.ui.jobs.resultMetricSummary
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

private const val TAG = "AskViewModel"
private const val FAB_STATUS_INITIAL_DELAY_MS = 1_800L
private const val FAB_STATUS_POLL_INTERVAL_MS = 2_500L
private const val FAB_STATUS_MAX_ATTEMPTS = 6
private const val MOBILE_SESSION_SAVE_DEBOUNCE_MS = 350L
private val FAB_TERMINAL_STATUSES = setOf("completed", "complete", "failed", "error", "cancelled", "canceled")

private fun JobFamily.toFabOp(): com.axon.app.ui.fab.FabOp = when (this) {
    JobFamily.Crawl -> com.axon.app.ui.fab.FabOp.Crawl
    JobFamily.Embed -> com.axon.app.ui.fab.FabOp.Embed
    JobFamily.Extract -> com.axon.app.ui.fab.FabOp.Extract
    JobFamily.Ingest -> com.axon.app.ui.fab.FabOp.Ingest
}

class AskViewModel(app: Application) : AndroidViewModel(app) {
    internal val container = (app as AxonApp).container

    internal val _uiState = MutableStateFlow<AskUiState>(AskUiState.Idle)
    val uiState: StateFlow<AskUiState> = _uiState.asStateFlow()

    internal val _mode = MutableStateFlow(ConversationMode.Ask)
    val mode: StateFlow<ConversationMode> = _mode.asStateFlow()

    private val _historyReady = MutableStateFlow(false)
    val historyReady: StateFlow<Boolean> = _historyReady.asStateFlow()

    val history = container.axonRepository.recentHistory()
        .onEach { _historyReady.value = true }
        .stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    internal val _turns = MutableStateFlow<List<AskTurn>>(emptyList())
    val turns: StateFlow<List<AskTurn>> = _turns.asStateFlow()

    internal val _chatItems = MutableStateFlow<List<ChatItem>>(emptyList())
    val chatItems: StateFlow<List<ChatItem>> = _chatItems.asStateFlow()

    /**
     * In-flight ask coroutine. Tracked so a second `ask()` call cancels the
     * prior stream — without this, repeated Asks pile up parallel SSE
     * connections, blocked OkHttp IO threads (readLine never returns until
     * STREAM_READ_TIMEOUT_SECONDS = 300s), and interleaved [_uiState] writes.
     * The user-visible symptom is an app that "hangs" and then force-closes.
     */
    internal var askJob: Job? = null

    /**
     * Attachment text of the most recent [ask] call. Attachments are intentionally
     * never stored in [_turns]/history (they'd leak into later follow-ups), so we
     * remember the latest one here to let [regenerateLast] re-run the same input.
     * Regenerate always targets the most recent user message, so reusing the most
     * recent attachment is correct.
     */
    internal var lastAttachment: String? = null

    /**
     * Whether the most recent [ask] appended a follow-up turn. Set false at the
     * start of every ask and true only where [appendTurn] runs (the Done and
     * truncation-fallback paths). [regenerateLast] consults this instead of
     * sniffing the answer text, so a partial-then-stopped answer (whose frozen
     * text is neither "Error:" nor "Stopped.") doesn't fool it into evicting the
     * previous good turn.
     */
    internal var lastAskProducedTurn = false
    private val emittedOperationContexts = mutableSetOf<String>()
    private var currentSessionId: String = newSessionId()
    private var createdAtMs: Long = System.currentTimeMillis()
    private var restoringSession = false
    private var persistSessionJob: Job? = null

    /** Drops all in-VM turns. Called by OperationsScreen on mode-switch away from Ask. */
    fun clearFollowUp() {
        _turns.value = emptyList()
        emittedOperationContexts.clear()
    }

    fun startNewSession() {
        cancelActiveSessionJobs()
        currentSessionId = newSessionId()
        createdAtMs = System.currentTimeMillis()
        restoringSession = true
        _chatItems.value = emptyList()
        _turns.value = emptyList()
        emittedOperationContexts.clear()
        lastAttachment = null
        lastAskProducedTurn = false
        _uiState.value = AskUiState.Idle
        restoringSession = false
    }

    fun loadSession(sessionId: String) {
        if (sessionId.isBlank() || sessionId == "new") {
            startNewSession()
            return
        }
        viewModelScope.launch {
            val session = container.axonRepository.getMobileSession(sessionId).getOrElse { cause ->
                Log.w(TAG, "Failed to load mobile session $sessionId", cause)
                _uiState.value = AskUiState.Error(
                    cause.message ?: "Could not load this chat session. Check your connection and sign in again.",
                )
                return@launch
            }
            cancelActiveSessionJobs()
            restoringSession = true
            currentSessionId = session.id
            createdAtMs = session.createdAt
            val items = session.items.mapNotNull { it.toChatItem() }
            _chatItems.value = items
            _turns.value = restoredTurns(items)
            emittedOperationContexts.clear()
            lastAttachment = null
            lastAskProducedTurn = false
            _uiState.value = AskUiState.Idle
            restoringSession = false
        }
    }

    /**
     * User-invoked stop: cancel the in-flight stream and freeze whatever partial
     * answer is already on screen. The last flushed text lives in [_chatItems], so
     * we just clear the streaming flag (cancelling the coroutine rethrows the
     * CancellationException, skipping the truncation-fallback in [ask]).
     */
    fun stopGeneration() {
        if (askJob?.isActive != true) return
        askJob?.cancel()
        askJob = null
        val items = _chatItems.value.toMutableList()
        val lastIdx = items.indexOfLast { it is ChatItem.AxonMsg }
        if (lastIdx >= 0) {
            val msg = items[lastIdx] as ChatItem.AxonMsg
            items[lastIdx] = msg.copy(text = msg.text.ifBlank { "Stopped." }, isStreaming = false)
            _chatItems.value = items
        }
        completeActivities()
        _uiState.value = AskUiState.Idle
    }

    fun setMode(mode: ConversationMode) {
        if (_mode.value == mode) return
        askJob?.cancel()
        _mode.value = mode
        _uiState.value = AskUiState.Idle
        _turns.value = emptyList()
        emittedOperationContexts.clear()
    }

    internal fun appendTurn(q: String, a: String) {
        _turns.value = (_turns.value + AskTurn(q, a.take(500))).takeLast(MAX_FOLLOW_UP_TURNS)
    }

    internal fun appendOperationContext(
        op: com.axon.app.ui.fab.FabOp,
        target: String,
        status: String,
        endpoint: String,
        jobId: String? = null,
        summary: String? = null,
        detail: String? = null,
    ) {
        val key = listOf(op.name, target, status, endpoint, jobId.orEmpty(), summary.orEmpty())
            .joinToString("|")
        if (!emittedOperationContexts.add(key)) return
        appendTurn(
            q = operationContextQuestion(op.label),
            a = operationContextAnswer(
                opLabel = op.label,
                target = target,
                status = status,
                endpoint = endpoint,
                jobId = jobId,
                summary = summary,
                detail = detail,
            ),
        )
    }

    internal fun appendItem(item: ChatItem) {
        _chatItems.value = _chatItems.value + item
        if (item !is ChatItem.AxonMsg || !item.isStreaming) persistCurrentSession()
    }

    internal fun appendOperationRequest(op: com.axon.app.ui.fab.FabOp, input: String) {
        appendItem(ChatItem.UserMsg("${op.label} · $input"))
    }

    internal fun replaceLastAxonMsg(text: String, isStreaming: Boolean = false) {
        val items = _chatItems.value.toMutableList()
        val lastIdx = items.indexOfLast { it is ChatItem.AxonMsg }
        if (lastIdx >= 0) {
            // copy() keeps the original timestamp so it doesn't reset on each stream flush.
            items[lastIdx] = (items[lastIdx] as ChatItem.AxonMsg).copy(text = text, isStreaming = isStreaming)
            _chatItems.value = items
        } else {
            _chatItems.value = items + ChatItem.AxonMsg(text, isStreaming)
        }
        if (!isStreaming) persistCurrentSession()
    }

    /**
     * Re-runs the most recent user question, replacing its answer. Drops the old
     * answer/activities and the stored turn, then [ask] re-appends the question
     * and streams a fresh response.
     */
    fun regenerateLast() {
        if (askJob?.isActive == true) return
        val items = _chatItems.value
        val userIdx = items.indexOfLast { it is ChatItem.UserMsg }
        if (userIdx < 0) return
        val query = (items[userIdx] as ChatItem.UserMsg).text

        // Only drop the last stored turn if the answer being regenerated actually
        // produced one. [lastAskProducedTurn] is true only where appendTurn ran
        // (Done / truncation-fallback); it stays false for errored or stopped
        // answers (including partial-then-stopped, which keeps non-blank text).
        // Dropping in those cases would wrongly evict the *previous* good turn.
        if (lastAskProducedTurn) {
            _turns.value = _turns.value.dropLast(1)
        }

        // toList() takes a defensive copy — subList returns a live view backed by the
        // snapshot list, which pins the dropped tail in memory and is fragile to mutate.
        _chatItems.value = items.subList(0, userIdx).toList()
        // Re-ask with the original attachment so an attachment-backed question
        // regenerates on the same input (attachments are never stored in turns/history).
        ask(query, attachment = lastAttachment)
    }

    internal fun appendOrUpdateActivity(phase: String, query: String) {
        val activity = activityForPhase(phase, query) ?: return
        val items = _chatItems.value.toMutableList()
        val idx = items.indexOfLast { it is ChatItem.Activity && it.name == activity.name && !it.done }
        if (idx >= 0) {
            items[idx] = activity
            _chatItems.value = items
        } else {
            val placeholderIdx = items.indexOfLast { it is ChatItem.AxonMsg && it.isStreaming && it.text.isBlank() }
            if (placeholderIdx >= 0) {
                items.add(placeholderIdx, activity)
                _chatItems.value = items
            } else {
                _chatItems.value = items + activity
            }
        }
    }

    internal fun completeActivities(persist: Boolean = true) {
        val items = _chatItems.value.map { item ->
            if (item is ChatItem.Activity && !item.done) {
                item.copy(result = "done", done = true)
            } else {
                item
            }
        }
        _chatItems.value = items
        if (persist) persistCurrentSession()
    }

    internal fun replaceLastAxonItem(item: ChatItem) {
        val items = _chatItems.value.toMutableList()
        val lastIdx = items.indexOfLast { it is ChatItem.AxonMsg }
        if (lastIdx >= 0) {
            items[lastIdx] = item
            _chatItems.value = items
        } else {
            _chatItems.value = items + item
        }
        persistCurrentSession()
    }

    internal fun updateInjection(jobId: String, transform: (ChatItem.Injection) -> ChatItem.Injection) {
        val items = _chatItems.value.toMutableList()
        val idx = items.indexOfLast { it is ChatItem.Injection && it.jobId == jobId }
        if (idx >= 0) {
            items[idx] = transform(items[idx] as ChatItem.Injection)
            _chatItems.value = items
            persistCurrentSession()
        }
    }

    private fun injectionTarget(jobId: String): String =
        (_chatItems.value.lastOrNull { it is ChatItem.Injection && it.jobId == jobId } as? ChatItem.Injection)
            ?.target
            ?: jobId

    internal fun pollCrawlOnce(jobId: String) {
        viewModelScope.launch {
            delay(FAB_STATUS_INITIAL_DELAY_MS)
            var everSucceeded = false
            repeat(FAB_STATUS_MAX_ATTEMPTS) { attempt ->
                val terminal = container.axonRepository.crawlStatus(jobId).fold(
                    onSuccess = { status ->
                        everSucceeded = true
                        val readableStatus = status.status.replaceFirstChar { it.titlecase() }
                        val pages = status.pagesCrawled
                        updateInjection(jobId) { item ->
                            item.copy(
                                status = readableStatus,
                                pageCount = pages,
                                detail = when {
                                    status.serverError != null -> "Crawl reported an error: ${status.serverError}"
                                    pages != null -> "Crawl has indexed $pages ${if (pages == 1) "page" else "pages"} from this target."
                                    else -> "Crawl is ${status.status.lowercase()}. Jobs will continue updating as workers process the target."
                                },
                            )
                        }
                        if (status.status.lowercase() in FAB_TERMINAL_STATUSES || pages != null) {
                            appendOperationContext(
                                op = com.axon.app.ui.fab.FabOp.Crawl,
                                target = injectionTarget(jobId),
                                status = readableStatus,
                                endpoint = "GET /v1/crawl/$jobId",
                                jobId = jobId,
                                summary = pages?.let { "%,d pages crawled".format(it) },
                                detail = status.serverError ?: "Crawl status is ${status.status.lowercase()}. Use Axon query/retrieve/ask over the indexed target for follow-up.",
                            )
                        }
                        status.status.lowercase() in FAB_TERMINAL_STATUSES
                    },
                    onFailure = { false },
                )
                if (terminal) return@launch
                if (attempt == FAB_STATUS_MAX_ATTEMPTS - 1) {
                    finishStalePoll(jobId, everSucceeded)
                    return@launch
                }
                delay(FAB_STATUS_POLL_INTERVAL_MS)
            }
        }
    }

    /**
     * After the poll loop gives up: if not a single status request ever
     * succeeded, the chip would otherwise stay frozen on its initial status with
     * no explanation — surface that the poller couldn't reach the status.
     */
    private fun finishStalePoll(jobId: String, everSucceeded: Boolean) {
        if (everSucceeded) return
        updateInjection(jobId) { item ->
            item.copy(detail = "Couldn't reach job status — track it from Jobs.")
        }
    }

    internal fun pollJobOnce(kind: JobFamily, jobId: String) {
        viewModelScope.launch {
            delay(FAB_STATUS_INITIAL_DELAY_MS)
            var everSucceeded = false
            repeat(FAB_STATUS_MAX_ATTEMPTS) { attempt ->
                val terminal = container.axonRepository.getJob(kind, jobId).fold(
                    onSuccess = { job ->
                        everSucceeded = true
                        val readableStatus = job.status.replaceFirstChar { it.titlecase() }
                        updateInjection(jobId) { item ->
                            item.copy(
                                status = readableStatus,
                                detail = when {
                                    job.errorText != null -> "${kind.label()} reported an error: ${job.errorText}"
                                    else -> "${kind.label()} is ${job.status.lowercase()}. Jobs will show completion, errors, and indexed output as workers process the target."
                                },
                            )
                        }
                        if (job.status.lowercase() in FAB_TERMINAL_STATUSES) {
                            appendOperationContext(
                                op = kind.toFabOp(),
                                target = job.target ?: job.url ?: job.id,
                                status = readableStatus,
                                endpoint = "GET /v1/${kind.name.lowercase()}/$jobId",
                                jobId = jobId,
                                summary = resultMetricSummary(job.resultJson),
                                detail = job.errorText ?: "${kind.label()} status is ${job.status.lowercase()}. Use Axon query/retrieve/ask over indexed output for follow-up.",
                            )
                        }
                        job.status.lowercase() in FAB_TERMINAL_STATUSES
                    },
                    onFailure = { false },
                )
                if (terminal) return@launch
                if (attempt == FAB_STATUS_MAX_ATTEMPTS - 1) {
                    finishStalePoll(jobId, everSucceeded)
                    return@launch
                }
                delay(FAB_STATUS_POLL_INTERVAL_MS)
            }
        }
    }

    internal suspend fun recordRecentJob(jobId: String, kind: String, target: String) {
        runCatching {
            container.recentJobs.add(
                RecentJob(
                    jobId = jobId,
                    kind = kind,
                    target = target,
                    submittedAt = System.currentTimeMillis(),
                ),
            )
        }.onFailure { Log.w(TAG, "Failed to record recent $kind job $jobId", it) }
    }

    fun ask(query: String, attachment: String? = null) = askFromQuery(query, attachment)

    fun submitFabOp(op: com.axon.app.ui.fab.FabOp, input: String) = submitFabOperation(op, input)

    private fun persistCurrentSession() {
        if (restoringSession) return
        persistSessionJob?.cancel()
        persistSessionJob = viewModelScope.launch {
            delay(MOBILE_SESSION_SAVE_DEBOUNCE_MS)
            val items = _chatItems.value
            if (items.isEmpty()) return@launch
            val now = System.currentTimeMillis()
            val session = buildMobileSessionDto(
                sessionId = currentSessionId,
                createdAt = createdAtMs,
                updatedAt = now,
                items = items,
            )
            container.axonRepository.upsertMobileSession(session).onFailure { cause ->
                Log.w(TAG, "Failed to save mobile session ${session.id}", cause)
                if (_uiState.value is AskUiState.Idle) {
                    _uiState.value = AskUiState.Error(
                        cause.message ?: "Could not save this chat session. Check your connection and sign in again.",
                    )
                }
            }
        }
    }

    private fun cancelActiveSessionJobs() {
        askJob?.cancel()
        persistSessionJob?.cancel()
        askJob = null
        persistSessionJob = null
    }
}
