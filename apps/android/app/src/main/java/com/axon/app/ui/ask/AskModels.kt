package com.axon.app.ui.ask

import com.axon.app.data.repository.AskResultUi

/** A single completed Q/A turn kept in-VM for follow-up context injection. */
data class AskTurn(val question: String, val answer: String)

enum class ConversationMode(val label: String) {
    Ask("Ask"),
    Chat("Chat"),
}

/** Maximum prior turns inlined into the next ask. Matches CLI's MAX_FOLLOW_UP_TURNS=6. */
internal const val MAX_FOLLOW_UP_TURNS = 6
internal const val STREAM_UI_FLUSH_MS = 50L
private const val CHAT_PREVIEW_CHARS = 2_000
internal const val HIT_SNIPPET_CHARS = 220
internal const val COMPACT_HIT_LIMIT = 2
internal const val COMPACT_HIT_TITLE_CHARS = 92
internal const val COMPACT_HIT_URL_CHARS = 88
internal const val COMPACT_HIT_SNIPPET_CHARS = 110

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

internal fun operationContextQuestion(opLabel: String): String =
    "Axon mobile operation: $opLabel"

internal fun operationContextAnswer(
    opLabel: String,
    target: String,
    status: String,
    endpoint: String,
    jobId: String? = null,
    summary: String? = null,
    detail: String? = null,
): String {
    val lines = mutableListOf(
        "Operation: $opLabel",
        "Target: $target",
        "Status: $status",
        "Endpoint: $endpoint",
        "Agent instruction: This content/job was produced by Axon. When answering follow-up questions about it, use the Axon knowledge base and load the axon or axon:using-axon skill if available.",
    )
    jobId?.takeIf { it.isNotBlank() }?.let { lines += "Job ID: $it" }
    summary?.takeIf { it.isNotBlank() }?.let { lines += "Summary: $it" }
    detail?.takeIf { it.isNotBlank() }?.let { lines += "Detail: $it" }
    return lines.joinToString("\n")
}

internal fun resolvedDoneAnswer(doneAnswer: String, accumulatedAnswer: String): String =
    doneAnswer.ifBlank { accumulatedAnswer }

internal fun previewText(value: String, limit: Int = CHAT_PREVIEW_CHARS): String =
    if (value.length <= limit) value else value.take(limit).trimEnd() + "\n\n…truncated in chat"

internal fun compactSingleLine(value: String, limit: Int): String {
    val oneLine = value.trim().replace(Regex("\\s+"), " ")
    if (oneLine.length <= limit) return oneLine
    val cut = oneLine.take(limit).trimEnd()
    val wordBoundaryCut = cut.substringBeforeLast(" ", missingDelimiterValue = cut)
        .takeIf { it.length >= limit / 2 }
        ?: cut
    return wordBoundaryCut + "..."
}

sealed interface AskUiState {
    data object Idle : AskUiState
    /** Waiting for the first SSE event (retrieval phase). */
    data object Loading : AskUiState
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

sealed interface ChatItem {
    // NOTE: `timestamp` on UserMsg/AxonMsg is display-only presentation metadata
    // (rendered as the message time). Item identity and list keying go through
    // [stableChatItemKey], NOT structural equality, so the wall-clock default is
    // safe — do not use `==`/`hashCode`/remember-keys on these by value, or the
    // clock will leak into identity. It is a constructor property (rather than a
    // body val) so `copy()` preserves it across streaming flushes.
    data class UserMsg(
        val text: String,
        val timestamp: Long = System.currentTimeMillis(),
    ) : ChatItem
    data class AxonMsg(
        val text: String,
        val isStreaming: Boolean = false,
        val timestamp: Long = System.currentTimeMillis(),
    ) : ChatItem
    data class Activity(
        val name: String,
        val arg: String,
        val result: String,
        val done: Boolean = false,
    ) : ChatItem
    data class ActionResult(
        val op: com.axon.app.ui.fab.FabOp,
        val target: String,
        val status: String,
        val endpoint: String,
        val summary: String,
        val body: String,
    ) : ChatItem
    data class Injection(
        val op: com.axon.app.ui.fab.FabOp,
        val target: String,
        val jobId: String? = null,
        val pageCount: Int? = null,
        val chunkCount: Int? = null,
        val status: String = "202 Accepted",
        val endpoint: String = "POST /v1/{operation}",
        val detail: String = "Job submitted. Open Jobs to monitor status, retry, or inspect details.",
    ) : ChatItem
}

internal fun stableChatItemKey(index: Int, item: ChatItem): String = when (item) {
    is ChatItem.UserMsg -> "user-$index-${item.text.take(32)}"
    is ChatItem.AxonMsg -> "axon-$index"
    is ChatItem.Activity -> "activity-$index-${item.name}-${item.arg}"
    is ChatItem.ActionResult -> "result-$index-${item.op.name}-${item.target.take(32)}"
    is ChatItem.Injection -> "injection-$index-${item.op.name}-${item.jobId ?: item.target}"
}
internal fun scrapeSummary(markdown: String): String {
    val words = markdown.splitToSequence(Regex("\\s+")).filter { it.isNotBlank() }.count()
    val chars = markdown.length
    return "${"%,d".format(words)} words · ${"%,d".format(chars)} chars"
}

internal fun humanMarkdownPreview(markdown: String): String =
    markdown
        .replace(Regex("""\[(.+?)]\((.+?)\)"""), "$1")
        .lineSequence()
        .map { line ->
            line.trim()
                .removePrefix("#")
                .removePrefix("#")
                .removePrefix("#")
                .trim()
        }
        .filter { it.isNotBlank() }
        .fold(mutableListOf<String>()) { acc, line ->
            if (acc.lastOrNull()?.equals(line, ignoreCase = true) != true) acc += line
            acc
        }
        .joinToString("\n\n")


internal fun activityForPhase(phase: String, query: String): ChatItem.Activity? {
    val normalized = phase.trim().lowercase()
    if (normalized.isBlank()) return null
    val arg = displayUserText(query).take(22).trim().ifBlank { "query" }
    return when {
        normalized.contains("retriev") -> ChatItem.Activity("retrieve", "$arg · k=8", "running")
        normalized.contains("search") -> ChatItem.Activity("search", arg, "running")
        normalized.contains("synth") || normalized.contains("generat") || normalized.contains("answer") ->
            ChatItem.Activity("ask", arg, "streaming")
        else -> ChatItem.Activity(normalized.replace('_', ' ').take(18), arg, "running")
    }
}
