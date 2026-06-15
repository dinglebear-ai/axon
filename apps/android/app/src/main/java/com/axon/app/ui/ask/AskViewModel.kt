package com.axon.app.ui.ask

import android.app.Application
import android.os.SystemClock
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.axon.app.AxonApp
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.remote.AskStreamEvent
import com.axon.app.data.repository.JobFamily
import com.axon.app.data.repository.AskResultUi
import com.axon.app.data.repository.RecentJob
import com.axon.app.data.util.UrlValidator
import com.axon.app.ui.ingest.IngestSource
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

private const val FAB_STATUS_INITIAL_DELAY_MS = 1_800L
private const val FAB_STATUS_POLL_INTERVAL_MS = 2_500L
private const val FAB_STATUS_MAX_ATTEMPTS = 6
private val FAB_TERMINAL_STATUSES = setOf("completed", "complete", "failed", "error", "cancelled", "canceled")

internal fun inferFabIngestSource(input: String): Result<IngestSource> {
    val target = input.trim()
    if (target.isBlank()) {
        return Result.failure(IllegalArgumentException("Target is required"))
    }

    if (target.startsWith("github/", ignoreCase = true)) return Result.success(IngestSource.Github)
    if (target.startsWith("gitlab/", ignoreCase = true)) return Result.success(IngestSource.Gitlab)
    if (target.startsWith("r/", ignoreCase = true)) return Result.success(IngestSource.Reddit)

    val host = UrlValidator.hostOrNull(target)
    val source = when {
        host == null -> IngestSource.Git
        IngestSource.Github.matchesHost(host) -> IngestSource.Github
        IngestSource.Gitlab.matchesHost(host) -> IngestSource.Gitlab
        IngestSource.Reddit.matchesHost(host) -> IngestSource.Reddit
        IngestSource.Youtube.matchesHost(host) -> IngestSource.Youtube
        else -> {
            val lookalikeToken = knownIngestHostToken(host) ?: return Result.success(IngestSource.Git)
            return Result.failure(
                IllegalArgumentException("Unsupported lookalike host: $lookalikeToken must be the registrable host or a real subdomain"),
            )
        }
    }

    source.validate(target)?.let { reason ->
        return Result.failure(IllegalArgumentException(reason))
    }
    return Result.success(source)
}

private fun knownIngestHostToken(host: String): String? =
    listOf("github.com", "gitlab.com", "reddit.com", "youtube.com", "youtu.be")
        .firstOrNull { host.contains(it) }

class AskViewModel(app: Application) : AndroidViewModel(app) {
    private val container = (app as AxonApp).container

    private val _uiState = MutableStateFlow<AskUiState>(AskUiState.Idle)
    val uiState: StateFlow<AskUiState> = _uiState.asStateFlow()

    private val _mode = MutableStateFlow(ConversationMode.Ask)
    val mode: StateFlow<ConversationMode> = _mode.asStateFlow()

    private val _historyReady = MutableStateFlow(false)
    val historyReady: StateFlow<Boolean> = _historyReady.asStateFlow()

    val history = container.axonRepository.recentHistory()
        .onEach { _historyReady.value = true }
        .stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    private val _turns = MutableStateFlow<List<AskTurn>>(emptyList())
    val turns: StateFlow<List<AskTurn>> = _turns.asStateFlow()

    private val _chatItems = MutableStateFlow<List<ChatItem>>(emptyList())
    val chatItems: StateFlow<List<ChatItem>> = _chatItems.asStateFlow()

    /**
     * In-flight ask coroutine. Tracked so a second `ask()` call cancels the
     * prior stream — without this, repeated Asks pile up parallel SSE
     * connections, blocked OkHttp IO threads (readLine never returns until
     * STREAM_READ_TIMEOUT_SECONDS = 300s), and interleaved [_uiState] writes.
     * The user-visible symptom is an app that "hangs" and then force-closes.
     */
    private var askJob: Job? = null

    /**
     * Attachment text of the most recent [ask] call. Attachments are intentionally
     * never stored in [_turns]/history (they'd leak into later follow-ups), so we
     * remember the latest one here to let [regenerateLast] re-run the same input.
     * Regenerate always targets the most recent user message, so reusing the most
     * recent attachment is correct.
     */
    private var lastAttachment: String? = null

    /** Drops all in-VM turns. Called by OperationsScreen on mode-switch away from Ask. */
    fun clearFollowUp() { _turns.value = emptyList() }

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
    }

    private fun appendTurn(q: String, a: String) {
        _turns.value = (_turns.value + AskTurn(q, a.take(500))).takeLast(MAX_FOLLOW_UP_TURNS)
    }

    private fun appendItem(item: ChatItem) {
        _chatItems.value = _chatItems.value + item
    }

    private fun appendOperationRequest(op: com.axon.app.ui.fab.FabOp, input: String) {
        appendItem(ChatItem.UserMsg("${op.label} · $input"))
    }

    private fun replaceLastAxonMsg(text: String, isStreaming: Boolean = false) {
        val items = _chatItems.value.toMutableList()
        val lastIdx = items.indexOfLast { it is ChatItem.AxonMsg }
        if (lastIdx >= 0) {
            // copy() keeps the original timestamp so it doesn't reset on each stream flush.
            items[lastIdx] = (items[lastIdx] as ChatItem.AxonMsg).copy(text = text, isStreaming = isStreaming)
            _chatItems.value = items
        } else {
            _chatItems.value = items + ChatItem.AxonMsg(text, isStreaming)
        }
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
        // produced one. A turn is appended (appendTurn) only on a SUCCESSFUL ask
        // (Done / truncation-fallback) — never when the last answer errored ("Error: …")
        // or was stopped ("Stopped." from stopGeneration). If we dropLast() in those
        // cases we'd wrongly evict the *previous* good turn and corrupt follow-up context.
        val lastAnswer = items.drop(userIdx + 1).filterIsInstance<ChatItem.AxonMsg>().lastOrNull()
        val producedTurn = lastAnswer != null &&
            !lastAnswer.text.startsWith("Error:") &&
            lastAnswer.text != "Stopped."
        if (producedTurn) {
            _turns.value = _turns.value.dropLast(1)
        }

        // toList() takes a defensive copy — subList returns a live view backed by the
        // snapshot list, which pins the dropped tail in memory and is fragile to mutate.
        _chatItems.value = items.subList(0, userIdx).toList()
        // Re-ask with the original attachment so an attachment-backed question
        // regenerates on the same input (attachments are never stored in turns/history).
        ask(query, attachment = lastAttachment)
    }

    private fun appendOrUpdateActivity(phase: String, query: String) {
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

    private fun completeActivities() {
        val items = _chatItems.value.map { item ->
            if (item is ChatItem.Activity && !item.done) {
                item.copy(result = "done", done = true)
            } else {
                item
            }
        }
        _chatItems.value = items
    }

    private fun replaceLastAxonItem(item: ChatItem) {
        val items = _chatItems.value.toMutableList()
        val lastIdx = items.indexOfLast { it is ChatItem.AxonMsg }
        if (lastIdx >= 0) {
            items[lastIdx] = item
            _chatItems.value = items
        } else {
            _chatItems.value = items + item
        }
    }

    private fun updateInjection(jobId: String, transform: (ChatItem.Injection) -> ChatItem.Injection) {
        val items = _chatItems.value.toMutableList()
        val idx = items.indexOfLast { it is ChatItem.Injection && it.jobId == jobId }
        if (idx >= 0) {
            items[idx] = transform(items[idx] as ChatItem.Injection)
            _chatItems.value = items
        }
    }

    private fun pollCrawlOnce(jobId: String) {
        viewModelScope.launch {
            delay(FAB_STATUS_INITIAL_DELAY_MS)
            repeat(FAB_STATUS_MAX_ATTEMPTS) { attempt ->
                val terminal = container.axonRepository.crawlStatus(jobId).fold(
                    onSuccess = { status ->
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
                        status.status.lowercase() in FAB_TERMINAL_STATUSES
                    },
                    onFailure = { false },
                )
                if (terminal || attempt == FAB_STATUS_MAX_ATTEMPTS - 1) return@launch
                delay(FAB_STATUS_POLL_INTERVAL_MS)
            }
        }
    }

    private fun pollJobOnce(kind: JobFamily, jobId: String) {
        viewModelScope.launch {
            delay(FAB_STATUS_INITIAL_DELAY_MS)
            repeat(FAB_STATUS_MAX_ATTEMPTS) { attempt ->
                val terminal = container.axonRepository.getJob(kind, jobId).fold(
                    onSuccess = { job ->
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
                        job.status.lowercase() in FAB_TERMINAL_STATUSES
                    },
                    onFailure = { false },
                )
                if (terminal || attempt == FAB_STATUS_MAX_ATTEMPTS - 1) return@launch
                delay(FAB_STATUS_POLL_INTERVAL_MS)
            }
        }
    }

    private suspend fun recordRecentJob(jobId: String, kind: String, target: String) {
        runCatching {
            container.recentJobs.add(
                RecentJob(
                    jobId = jobId,
                    kind = kind,
                    target = target,
                    submittedAt = System.currentTimeMillis(),
                ),
            )
        }
    }

    fun ask(query: String, attachment: String? = null) {
        if (query.isBlank()) return
        // Remember the attachment of the question being asked so regenerateLast()
        // can re-run the same input (attachment text is never stored in turns/history).
        lastAttachment = attachment
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
                                replaceLastAxonMsg("Error: No response received from server", isStreaming = false)
                                return@collect
                            }
                            val result = AskResultUi(query = query, answer = finalAnswer, timingMs = null)
                            val saved = container.axonRepository.recordAskHistory(
                                AskHistoryEntry(query = result.query, answer = result.answer)
                            )
                            _uiState.value = AskUiState.Success(
                                result = result,
                                historyWarning = if (!saved) "Answer shown, but history could not be saved (storage may be full)." else null,
                            )
                            completeActivities()
                            replaceLastAxonMsg(finalAnswer, isStreaming = false)
                            appendTurn(q = query, a = finalAnswer)
                        }
                        is AskStreamEvent.Error -> {
                            if (accumulated.isNotBlank()) flushStreaming(force = true)
                            _uiState.value = AskUiState.Error(event.message)
                            replaceLastAxonMsg("Error: ${event.message}", isStreaming = false)
                        }
                    }
                }
            }.onFailure { err ->
                // Re-throw CancellationException so structured cancellation propagates correctly.
                // Any other exception is surfaced as an error state.
                if (err is CancellationException) throw err
                _uiState.value = AskUiState.Error(err.message ?: "Unknown error")
                replaceLastAxonMsg("Error: ${err.message ?: "Unknown error"}", isStreaming = false)
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
                    completeActivities()
                    replaceLastAxonMsg(finalAnswer, isStreaming = false)
                    appendTurn(q = query, a = finalAnswer)
                } else {
                    _uiState.value = AskUiState.Error("No response received from server")
                    replaceLastAxonMsg("Error: No response received from server", isStreaming = false)
                }
            }
        }
    }

    fun submitFabOp(op: com.axon.app.ui.fab.FabOp, input: String) {
        viewModelScope.launch {
            val repo = container.axonRepository
            when (op) {
                com.axon.app.ui.fab.FabOp.Scrape -> {
                    appendOperationRequest(op, input)
                    appendItem(ChatItem.AxonMsg("", isStreaming = true))
                    repo.scrape(url = input).fold(
                        onSuccess = { r ->
                            replaceLastAxonItem(
                                ChatItem.ActionResult(
                                    op = op,
                                    target = r.url.ifBlank { input },
                                    status = "200 OK",
                                    endpoint = "POST /v1/scrape",
                                    summary = scrapeSummary(r.markdown),
                                    body = previewText(humanMarkdownPreview(r.markdown), limit = 900),
                                ),
                            )
                        },
                        onFailure = { e -> replaceLastAxonMsg("Error: ${e.message}") },
                    )
                }
                com.axon.app.ui.fab.FabOp.Extract -> {
                    appendOperationRequest(com.axon.app.ui.fab.FabOp.Extract, input)
                    repo.extractStart(url = input).fold(
                        onSuccess = { jobId ->
                            recordRecentJob(jobId, kind = "extract", target = input)
                            appendItem(
                                ChatItem.Injection(
                                    op = com.axon.app.ui.fab.FabOp.Extract,
                                    target = input,
                                    jobId = jobId,
                                    endpoint = "POST /v1/extract",
                                    detail = "Extraction is queued. Jobs will show schema output and any server errors.",
                                ),
                            )
                            pollJobOnce(JobFamily.Extract, jobId)
                        },
                        onFailure = { e -> appendItem(ChatItem.AxonMsg("Extract failed: ${e.message}")) },
                    )
                }
                com.axon.app.ui.fab.FabOp.Embed -> {
                    appendOperationRequest(com.axon.app.ui.fab.FabOp.Embed, input)
                    repo.embedStart(input = input).fold(
                        onSuccess = { jobId ->
                            recordRecentJob(jobId, kind = "embed", target = input)
                            appendItem(
                                ChatItem.Injection(
                                    op = com.axon.app.ui.fab.FabOp.Embed,
                                    target = input,
                                    jobId = jobId,
                                    endpoint = "POST /v1/embed",
                                    detail = "Embed is queued. Chunks, document count, and errors are tracked in Jobs.",
                                ),
                            )
                            pollJobOnce(JobFamily.Embed, jobId)
                        },
                        onFailure = { e -> appendItem(ChatItem.AxonMsg("Embed failed: ${e.message}")) },
                    )
                }
                com.axon.app.ui.fab.FabOp.Research -> {
                    appendOperationRequest(com.axon.app.ui.fab.FabOp.Research, input)
                    appendItem(ChatItem.AxonMsg("", isStreaming = true))
                    repo.research(query = input).fold(
                        onSuccess = { r -> replaceLastAxonMsg(previewText(r.summary ?: "(no summary returned)")) },
                        onFailure = { e -> replaceLastAxonMsg("Error: ${e.message}") },
                    )
                }
                com.axon.app.ui.fab.FabOp.Query -> {
                    appendOperationRequest(com.axon.app.ui.fab.FabOp.Query, input)
                    appendItem(ChatItem.AxonMsg("", isStreaming = true))
                    repo.query(query = input).fold(
                        onSuccess = { hits ->
                            val text = hits.take(5).joinToString("\n\n") { h ->
                                "• ${h.url}\n  ${previewText(h.snippet, HIT_SNIPPET_CHARS)}"
                            }.ifBlank { "No results found." }
                            replaceLastAxonMsg(text)
                        },
                        onFailure = { e -> replaceLastAxonMsg("Error: ${e.message}") },
                    )
                }
                com.axon.app.ui.fab.FabOp.Search -> {
                    appendOperationRequest(com.axon.app.ui.fab.FabOp.Search, input)
                    appendItem(ChatItem.AxonMsg("", isStreaming = true))
                    repo.searchWeb(query = input).fold(
                        onSuccess = { r ->
                            val resultsText = r.results.take(5).joinToString("\n\n") { h ->
                                "• ${h.title}\n  ${h.url}\n  ${previewText(h.snippet.orEmpty(), HIT_SNIPPET_CHARS)}"
                            }.ifBlank { "No results found." }
                            val jobsText = r.crawlJobs.takeIf { it.isNotEmpty() }?.joinToString(
                                prefix = "\n\nQueued crawl jobs:\n",
                                separator = "\n",
                            ) { job -> "• ${job.jobId} — ${job.url}" }.orEmpty()
                            r.crawlJobs.forEach { job ->
                                recordRecentJob(job.jobId, kind = "crawl", target = job.url)
                            }
                            replaceLastAxonMsg(resultsText + jobsText)
                        },
                        onFailure = { e -> replaceLastAxonMsg("Error: ${e.message}") },
                    )
                }
                com.axon.app.ui.fab.FabOp.Map -> {
                    appendOperationRequest(com.axon.app.ui.fab.FabOp.Map, input)
                    appendItem(ChatItem.AxonMsg("", isStreaming = true))
                    repo.map(url = input).fold(
                        onSuccess = { r ->
                            val text = "Found ${r.total} URLs:\n" + r.urls.take(20).joinToString("\n") { "• $it" }
                            replaceLastAxonMsg(text)
                        },
                        onFailure = { e -> replaceLastAxonMsg("Error: ${e.message}") },
                    )
                }
                com.axon.app.ui.fab.FabOp.Retrieve -> {
                    appendOperationRequest(com.axon.app.ui.fab.FabOp.Retrieve, input)
                    appendItem(ChatItem.AxonMsg("", isStreaming = true))
                    repo.retrieve(url = input).fold(
                        onSuccess = { r -> replaceLastAxonMsg(previewText(r.content)) },
                        onFailure = { e -> replaceLastAxonMsg("Error: ${e.message}") },
                    )
                }
                com.axon.app.ui.fab.FabOp.Summarize -> {
                    appendOperationRequest(com.axon.app.ui.fab.FabOp.Summarize, input)
                    appendItem(ChatItem.AxonMsg("", isStreaming = true))
                    repo.summarize(urls = listOf(input)).fold(
                        onSuccess = { r -> replaceLastAxonMsg(previewText(r.summary)) },
                        onFailure = { e -> replaceLastAxonMsg("Error: ${e.message}") },
                    )
                }
                com.axon.app.ui.fab.FabOp.Crawl -> {
                    appendOperationRequest(com.axon.app.ui.fab.FabOp.Crawl, input)
                    repo.crawlSubmit(url = input).fold(
                        onSuccess = { jobId ->
                            recordRecentJob(jobId, kind = "crawl", target = input)
                            appendItem(
                                ChatItem.Injection(
                                    op = com.axon.app.ui.fab.FabOp.Crawl,
                                    target = input,
                                    jobId = jobId,
                                    endpoint = "POST /v1/crawl",
                                    detail = "Crawl is queued. Pages, errors, and completion state are pulled from the job endpoint.",
                                ),
                            )
                            pollCrawlOnce(jobId)
                        },
                        onFailure = { e ->
                            appendItem(
                                ChatItem.Injection(
                                    op = com.axon.app.ui.fab.FabOp.Crawl,
                                    target = input,
                                    jobId = null,
                                    status = "FAILED",
                                    endpoint = "POST /v1/crawl",
                                    detail = "Crawl failed: ${e.message ?: "unknown server error"}",
                                ),
                            )
                        },
                    )
                }
                com.axon.app.ui.fab.FabOp.Ingest -> {
                    appendOperationRequest(com.axon.app.ui.fab.FabOp.Ingest, input)
                    val sourceType = inferFabIngestSource(input).fold(
                        onSuccess = { it.wire },
                        onFailure = { e ->
                            appendItem(ChatItem.AxonMsg("Ingest failed: ${e.message ?: "invalid target"}"))
                            return@launch
                        },
                    )
                    repo.ingestStart(sourceType = sourceType, target = input).fold(
                        onSuccess = { jobId ->
                            recordRecentJob(jobId, kind = "ingest", target = input)
                            appendItem(
                                ChatItem.Injection(
                                    op = com.axon.app.ui.fab.FabOp.Ingest,
                                    target = input,
                                    jobId = jobId,
                                    endpoint = "POST /v1/ingest",
                                    detail = "Ingest is queued. Source discovery and embedding progress are tracked in Jobs.",
                                ),
                            )
                            pollJobOnce(JobFamily.Ingest, jobId)
                        },
                        onFailure = { e -> appendItem(ChatItem.AxonMsg("Ingest failed: ${e.message}")) },
                    )
                }
            }
        }
    }
}
