package com.axon.app.data.remote

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

private val errorJson = Json { ignoreUnknownKeys = true }

internal fun httpErrorMessage(code: Int, rawBody: String?, fallback: String): String {
    val detail = humanHttpBody(rawBody).ifBlank { fallback }
    return "HTTP $code: ${detail.take(200)}"
}

private fun humanHttpBody(rawBody: String?): String {
    val body = rawBody?.trim().orEmpty()
    if (body.isBlank()) return ""
    val parsed = runCatching { errorJson.parseToJsonElement(body) }.getOrNull()
    return parsed?.let(::humanHttpElement)?.ifBlank { body.stripJsonPunctuation() }
        ?: body.stripJsonPunctuation()
}

private fun humanHttpElement(element: JsonElement): String = when (element) {
    is JsonObject -> {
        val primary = listOf("message", "error", "detail", "details", "reason", "description")
            .firstNotNullOfOrNull { key -> element[key]?.let(::humanHttpElement)?.takeIf { it.isNotBlank() } }
        primary ?: element.entries
            .take(4)
            .joinToString(" · ") { (key, value) ->
                "${key.humanHttpLabel()}: ${humanHttpElement(value)}"
            }
    }
    is JsonArray -> element.take(4).joinToString(" · ") { humanHttpElement(it) }
    is JsonPrimitive -> element.contentOrNull.orEmpty()
    else -> ""
}

private fun String.humanHttpLabel(): String =
    replace('_', ' ')
        .replace('-', ' ')
        .trim()
        .replaceFirstChar { it.uppercase() }

private fun String.stripJsonPunctuation(): String =
    replace(Regex("[{}\\[\\]\"]"), "")
        .replace(Regex("\\s*,\\s*"), " · ")
        .replace(Regex("\\s*:\\s*"), ": ")
        .trim()
