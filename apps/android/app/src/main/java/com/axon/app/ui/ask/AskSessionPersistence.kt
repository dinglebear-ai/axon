package com.axon.app.ui.ask

import com.axon.app.data.remote.models.MobileChatItemDto
import com.axon.app.data.remote.models.MobileSessionDto
import com.axon.app.ui.fab.FabOp
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import java.util.UUID

internal fun newSessionId(): String = UUID.randomUUID().toString()

internal fun buildMobileSessionDto(
    sessionId: String,
    createdAt: Long,
    updatedAt: Long,
    items: List<ChatItem>,
): MobileSessionDto = MobileSessionDto(
    id = sessionId,
    title = sessionTitle(items),
    firstMessagePreview = firstMessagePreview(items),
    turnCount = items.count { it is ChatItem.UserMsg },
    injectedOpCount = items.count { it is ChatItem.Injection || it is ChatItem.ActionResult },
    createdAt = createdAt,
    updatedAt = updatedAt,
    items = items.map { it.toMobileDto(updatedAt) },
)

internal fun restoredTurns(items: List<ChatItem>): List<AskTurn> {
    val turns = mutableListOf<AskTurn>()
    var pendingQuestion: String? = null
    for (item in items) {
        when (item) {
            is ChatItem.UserMsg -> pendingQuestion = item.text
            is ChatItem.AxonMsg -> {
                val q = pendingQuestion ?: continue
                if (item.text.isNotBlank()) {
                    turns += AskTurn(q, item.text.take(500))
                }
                pendingQuestion = null
            }
            else -> Unit
        }
    }
    return turns.takeLast(MAX_FOLLOW_UP_TURNS)
}

private fun axonMessage(text: String?, timestamp: Long): ChatItem.AxonMsg? =
    text?.let { ChatItem.AxonMsg(it, isStreaming = false, timestamp = timestamp) }

internal fun MobileChatItemDto.toChatItem(): ChatItem? = when (kind) {
    "user" -> text?.let { ChatItem.UserMsg(it, timestamp = timestamp) }
    "axon" -> axonMessage(text, timestamp)
    "activity" -> payload.activityItem()
    "action_result" -> payload.actionResultItem()
    "injection" -> payload.injectionItem()
    else -> null
}

private fun JsonObject.activityItem(): ChatItem.Activity? {
    val name = stringPayload("name") ?: return null
    return ChatItem.Activity(
        name = name,
        arg = stringPayload("arg").orEmpty(),
        result = stringPayload("result").orEmpty(),
        done = booleanPayload("done") == true,
    )
}

private fun JsonObject.actionResultItem(): ChatItem.ActionResult? {
    val op = opPayload() ?: return null
    return ChatItem.ActionResult(
        op = op,
        target = stringPayload("target").orEmpty(),
        status = stringPayload("status").orEmpty(),
        endpoint = stringPayload("endpoint").orEmpty(),
        summary = stringPayload("summary").orEmpty(),
        body = stringPayload("body").orEmpty(),
    )
}

private fun JsonObject.injectionItem(): ChatItem.Injection? {
    val op = opPayload() ?: return null
    return ChatItem.Injection(
        op = op,
        target = stringPayload("target").orEmpty(),
        jobId = stringPayload("job_id"),
        pageCount = intPayload("page_count"),
        chunkCount = intPayload("chunk_count"),
        status = stringPayload("status").orEmpty(),
        endpoint = stringPayload("endpoint").orEmpty(),
        detail = stringPayload("detail").orEmpty(),
    )
}

private fun JsonObject.opPayload(): FabOp? =
    stringPayload("op")?.let { raw ->
        FabOp.entries.firstOrNull { it.name == raw }
    }

private fun JsonObject.stringPayload(key: String): String? =
    this[key]?.jsonPrimitive?.content?.takeIf { it.isNotBlank() }

private fun JsonObject.intPayload(key: String): Int? =
    this[key]?.jsonPrimitive?.intOrNull

private fun JsonObject.booleanPayload(key: String): Boolean? =
    this[key]?.jsonPrimitive?.booleanOrNull

private fun ChatItem.toMobileDto(defaultTimestamp: Long): MobileChatItemDto = when (this) {
    is ChatItem.UserMsg -> MobileChatItemDto(
        kind = "user",
        text = text,
        timestamp = timestamp,
    )
    is ChatItem.AxonMsg -> MobileChatItemDto(
        kind = "axon",
        text = text,
        timestamp = timestamp,
        payload = buildJsonObject { put("streaming", JsonPrimitive(isStreaming)) },
    )
    is ChatItem.Activity -> MobileChatItemDto(
        kind = "activity",
        text = listOf(name, arg, result).filter { it.isNotBlank() }.joinToString(" · "),
        timestamp = defaultTimestamp,
        payload = buildJsonObject {
            put("name", name)
            put("arg", arg)
            put("result", result)
            put("done", done)
        },
    )
    is ChatItem.ActionResult -> MobileChatItemDto(
        kind = "action_result",
        text = buildString {
            append(op.label)
            append(" · ")
            append(status)
            append("\n")
            append(summary)
            if (body.isNotBlank()) {
                append("\n\n")
                append(body)
            }
        },
        timestamp = defaultTimestamp,
        payload = buildJsonObject {
            put("op", op.name)
            put("target", target)
            put("status", status)
            put("endpoint", endpoint)
            put("summary", summary)
            put("body", body)
        },
    )
    is ChatItem.Injection -> MobileChatItemDto(
        kind = "injection",
        text = buildString {
            append(op.label)
            append(" · ")
            append(status)
            if (!jobId.isNullOrBlank()) append("\nJob: $jobId")
            append("\n")
            append(detail)
        },
        timestamp = defaultTimestamp,
        payload = buildJsonObject {
            put("op", op.name)
            put("target", target)
            jobId?.let { put("job_id", it) }
            pageCount?.let { put("page_count", it) }
            chunkCount?.let { put("chunk_count", it) }
            put("status", status)
            put("endpoint", endpoint)
            put("detail", detail)
        },
    )
}

private fun sessionTitle(items: List<ChatItem>): String =
    (items.firstOrNull { it is ChatItem.UserMsg } as? ChatItem.UserMsg)
        ?.text
        ?.cleanSessionText()
        ?.take(64)
        ?.ifBlank { null }
        ?: "New Session"

private fun firstMessagePreview(items: List<ChatItem>): String =
    (items.firstOrNull { it is ChatItem.UserMsg } as? ChatItem.UserMsg)
        ?.text
        ?.cleanSessionText()
        ?.take(180)
        ?: ""

private fun String.cleanSessionText(): String =
    replace(Regex("\\s+"), " ").trim()
