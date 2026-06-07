package com.axon.app.ui.common

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.longOrNull

private val humanJsonParser = Json {
    ignoreUnknownKeys = true
    isLenient = true
}

data class HumanJsonRow(
    val label: String,
    val value: String,
    val depth: Int = 0,
)

fun JsonElement.humanRows(maxRows: Int = 160): List<HumanJsonRow> {
    val rows = mutableListOf<HumanJsonRow>()
    fun visit(label: String, element: JsonElement, depth: Int) {
        if (rows.size >= maxRows) return
        when (element) {
            is JsonObject -> {
                if (label.isNotBlank()) rows += HumanJsonRow(humanLabel(label), "${element.size} fields", depth)
                element.entries
                    .sortedBy { it.key }
                    .forEach { (key, value) -> visit(key, value, depth + if (label.isNotBlank()) 1 else 0) }
            }
            is JsonArray -> {
                rows += HumanJsonRow(humanLabel(label), "${element.size} items", depth)
                element.take(24).forEachIndexed { index, value -> visit("Item ${index + 1}", value, depth + 1) }
            }
            is JsonPrimitive -> rows += HumanJsonRow(humanLabel(label), humanValue(element), depth)
            JsonNull -> rows += HumanJsonRow(humanLabel(label), "Not set", depth)
        }
    }
    visit("", this, 0)
    return rows.ifEmpty { listOf(HumanJsonRow("Status", "No details returned")) }
}

fun JsonElement.humanSummary(maxRows: Int = 6): String =
    humanRows(maxRows = maxRows)
        .take(maxRows)
        .joinToString(" · ") { row -> "${row.label}: ${row.value}" }

fun JsonElement.doctorServiceSummary(maxServices: Int = 6): String {
    val root = this as? JsonObject ?: return humanSummary(maxServices)
    val services = root["services"] as? JsonObject ?: return humanSummary(maxServices)
    val preferredOrder = listOf("qdrant", "tei", "chrome", "sqlite", "gemini_headless")
    val ordered = preferredOrder
        .mapNotNull { key -> services[key]?.let { key to it } } +
        services.entries
            .filterNot { it.key in preferredOrder }
            .sortedBy { it.key }
            .map { it.key to it.value }

    return ordered
        .take(maxServices)
        .map { (name, service) -> doctorServiceLine(name, service) }
        .joinToString("\n")
        .ifBlank { humanSummary(maxServices) }
}

private fun doctorServiceLine(name: String, service: JsonElement): String {
    val obj = service as? JsonObject ?: return "${humanLabel(name)} · ${humanValueForLine(service)}"
    val ok = obj.boolean("ok")
    val status = when (ok) {
        true -> "up"
        false -> "down"
        null -> if (obj.boolean("exists") == false || obj.boolean("configured") == false) "warn" else "ready"
    }
    val target = obj.string("effective_url")
        ?: obj.string("url")
        ?: obj.string("collection")
        ?: obj.string("path")
        ?: obj.string("command")
    val detail = obj.string("model")
        ?: obj.string("vector_mode")
        ?: obj.string("detail")
        ?: obj.string("summary")
    val parts = listOfNotNull(
        doctorServiceName(name),
        status,
        target?.compactEndpoint(),
        detail?.compactDetail(),
    ).filter { it.isNotBlank() }
    return parts.joinToString(" · ")
}

private fun doctorServiceName(name: String): String = when (name) {
    "qdrant" -> "axon-qdrant"
    "tei" -> "axon-tei"
    "chrome" -> "axon-chrome"
    "sqlite" -> "sqlite"
    "gemini_headless" -> "gemini"
    else -> humanLabel(name).replace(' ', '-').lowercase()
}

private fun JsonObject.string(key: String): String? =
    (this[key] as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotBlank() }

private fun JsonObject.boolean(key: String): Boolean? =
    (this[key] as? JsonPrimitive)?.booleanOrNull

private fun String.compactEndpoint(): String =
    removePrefix("http://")
        .removePrefix("https://")
        .trimEnd('/')

private fun String.compactDetail(): String =
    replace("http ", "HTTP ")
        .replace('_', ' ')
        .take(88)

private fun humanValueForLine(value: JsonElement): String = when (value) {
    is JsonPrimitive -> humanValue(value)
    is JsonArray -> "${value.size} items"
    is JsonObject -> "${value.size} fields"
    JsonNull -> "Not set"
}

fun humanizeJsonText(text: String, maxRows: Int = 24): String {
    val trimmed = text.trim()
    if (!trimmed.looksLikeJsonPayload()) return text

    return runCatching {
        humanJsonParser.parseToJsonElement(trimmed)
            .humanRows(maxRows = maxRows)
            .joinToString("\n") { row ->
                val indent = "  ".repeat(row.depth.coerceAtMost(3))
                "$indent${row.label}: ${row.value}"
            }
    }.getOrElse { text }
}

fun humanizeJsonFragmentText(text: String, maxRows: Int = 24): String {
    val trimmed = text.trim()
    val span = trimmed.findJsonPayloadSpan()
    val start = span?.first ?: -1
    if (start <= 0) return humanizeJsonText(text, maxRows)

    val prefix = trimmed.take(start).trimEnd()
    val end = span!!.last + 1
    val payload = trimmed.substring(start, end).trim()
    val suffix = trimmed.drop(end).trimStart()
    val humanPayload = humanizeJsonText(payload, maxRows)
    if (humanPayload == payload) return text

    return buildString {
        append(prefix)
        append('\n')
        append(humanPayload)
        if (suffix.isNotBlank()) {
            append('\n')
            append(suffix)
        }
    }
}

fun humanLabel(raw: String): String =
    raw
        .replace(Regex("([a-z])([A-Z])"), "$1 $2")
        .replace('_', ' ')
        .replace('-', ' ')
        .trim()
        .split(Regex("\\s+"))
        .filter { it.isNotBlank() }
        .joinToString(" ") { word -> word.replaceFirstChar { it.uppercase() } }

fun humanValue(value: JsonPrimitive): String =
    when {
        value is JsonNull -> "Not set"
        value.booleanOrNull != null -> if (value.booleanOrNull == true) "Yes" else "No"
        value.longOrNull != null -> "%,d".format(value.longOrNull)
        value.doubleOrNull != null -> formatDecimal(value.doubleOrNull ?: 0.0)
        else -> value.contentOrNull?.ifBlank { "Not set" } ?: "Not set"
    }

private fun formatDecimal(value: Double): String =
    if (value % 1.0 == 0.0) "%,.0f".format(value) else "%,.2f".format(value)

private fun String.looksLikeJsonPayload(): Boolean =
    (startsWith("{") && endsWith("}")) || (startsWith("[") && endsWith("]"))

private fun String.findJsonPayloadSpan(): IntRange? {
    val start = indexOfFirst { it == '{' || it == '[' }
    if (start < 0) return null

    val opening = this[start]
    val closing = if (opening == '{') '}' else ']'
    var depth = 0
    var inString = false
    var escaped = false
    for (index in start until length) {
        val char = this[index]
        if (escaped) {
            escaped = false
            continue
        }
        if (char == '\\' && inString) {
            escaped = true
            continue
        }
        if (char == '"') {
            inString = !inString
            continue
        }
        if (inString) continue
        when (char) {
            opening -> depth++
            closing -> {
                depth--
                if (depth == 0) return start..index
            }
        }
    }
    return null
}
