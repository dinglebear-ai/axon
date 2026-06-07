package com.axon.app.ui.jobs

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.CloudDownload
import androidx.compose.material.icons.rounded.DataObject
import androidx.compose.material.icons.rounded.TravelExplore
import androidx.compose.material.icons.rounded.Work
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.JobUi
import com.axon.app.ui.theme.AxonTheme
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.longOrNull
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

internal fun jobDisplayTarget(job: JobUi): String =
    job.url ?: job.target ?: job.id.take(12)

internal fun progressForStatus(status: String): Float = when (status.lowercase()) {
    "pending" -> 0.10f
    "running" -> 0.17f
    "processing" -> 0.45f
    "done", "completed" -> 1f
    else -> 0.08f
}

internal fun progressForJob(job: JobUi): Float {
    val fromResult = progressFromResult(job.resultJson)
    return fromResult ?: progressForStatus(job.status)
}

internal fun progressFromResult(result: JsonElement?): Float? {
    val obj = result as? JsonObject ?: return null
    val done = firstMetric(obj, "done", "fetched", "pages_crawled", "pages", "processed", "completed")
    val total = firstMetric(obj, "total", "queued", "page_count", "pages_total", "expected", "count")
    if (done == null || total == null || total <= 0L) return null
    return (done.toFloat() / total.toFloat()).coerceIn(0.02f, 1f)
}

internal fun firstMetric(obj: JsonObject, vararg keys: String): Long? {
    for (key in keys) {
        val value = obj[key]?.let { primitiveLong(it) }
        if (value != null) return value
    }
    for ((_, child) in obj) {
        val nested = child as? JsonObject ?: continue
        val value = firstMetric(nested, *keys)
        if (value != null) return value
    }
    return null
}

internal fun primitiveLong(element: JsonElement): Long? =
    (element as? JsonPrimitive)?.longOrNull

internal fun AxonClient.JobKind.label(): String = when (this) {
    AxonClient.JobKind.Crawl -> "Crawl"
    AxonClient.JobKind.Embed -> "Embed"
    AxonClient.JobKind.Extract -> "Extract"
    AxonClient.JobKind.Ingest -> "Ingest"
}

internal fun AxonClient.JobKind.drillTitle(): String = when (this) {
    AxonClient.JobKind.Crawl -> "Crawls"
    AxonClient.JobKind.Embed -> "Embeddings"
    AxonClient.JobKind.Extract -> "Extractions"
    AxonClient.JobKind.Ingest -> "Ingestions"
}

@Composable
internal fun jobTone(kind: AxonClient.JobKind?): Color = when (kind) {
    AxonClient.JobKind.Crawl -> AxonTheme.colors.accentPrimary
    AxonClient.JobKind.Embed -> AxonTheme.colors.accentPink
    AxonClient.JobKind.Extract -> AxonTheme.colors.orange
    AxonClient.JobKind.Ingest -> AxonTheme.colors.accentStrong
    null -> AxonTheme.colors.accentPrimary
}

@Composable
internal fun toneForKindName(kind: String): Color = when (kind.lowercase()) {
    "crawl" -> AxonTheme.colors.accentPrimary
    "embed" -> AxonTheme.colors.accentPink
    "extract" -> AxonTheme.colors.orange
    "ingest" -> AxonTheme.colors.accentStrong
    else -> AxonTheme.colors.accentPrimary
}

internal fun iconForKind(kind: AxonClient.JobKind?): ImageVector = when (kind) {
    AxonClient.JobKind.Crawl -> Icons.Rounded.TravelExplore
    AxonClient.JobKind.Embed -> Icons.Rounded.Work
    AxonClient.JobKind.Extract -> Icons.Rounded.DataObject
    AxonClient.JobKind.Ingest -> Icons.Rounded.CloudDownload
    null -> Icons.Rounded.Work
}

internal fun iconForKindName(kind: String): ImageVector = when (kind.lowercase()) {
    "crawl" -> Icons.Rounded.TravelExplore
    "embed" -> Icons.Rounded.Work
    "extract" -> Icons.Rounded.DataObject
    "ingest" -> Icons.Rounded.CloudDownload
    else -> Icons.Rounded.Work
}

internal fun shortTarget(target: String): String =
    target
        .removePrefix("https://")
        .removePrefix("http://")
        .trimEnd('/')
        .substringAfterLast("/home/axon/.axon/output/domains/", missingDelimiterValue = target.removePrefix("https://").removePrefix("http://").trimEnd('/'))
        .let { compact -> if (compact.length > 54) compact.take(51).trimEnd() + "..." else compact }
        .ifBlank { "job" }

internal fun jobProgressLabel(job: JobUi): String =
    listOfNotNull(
        resultMetricSummary(job.resultJson) ?: fallbackJobDetail(job),
        "job ${job.id.take(8)}",
    ).joinToString(" · ")

internal fun statusLabel(status: String): String = when (status.lowercase()) {
    "done", "completed", "success" -> "done"
    "processing" -> "running"
    "failed", "error" -> "failed"
    else -> status.lowercase()
}

internal fun fallbackJobDetail(job: JobUi): String = when (job.status.lowercase()) {
    "pending" -> "queued for ${job.kind?.label()?.lowercase() ?: "work"}"
    "running", "processing" -> "active ${job.kind?.label()?.lowercase() ?: "job"}"
    "done", "completed", "success" -> job.finishedAt?.let { "finished $it" } ?: "completed"
    "failed", "error" -> "failed"
    else -> job.status.lowercase().ifBlank { "job detail unavailable" }
}

internal fun resultMetricSummary(result: JsonElement?): String? {
    if (result == null) return null
    val metrics = mutableListOf<String>()
    val metricLabels = mutableSetOf<String>()
    fun addMetric(label: String, value: Long?) {
        if (value == null || value < 0 || metrics.size >= 3 || !metricLabels.add(label)) return
        val singular = label.removeSuffix("s")
        metrics += "%,d %s".format(value, if (value == 1L) singular else label)
    }
    fun visit(node: JsonElement, key: String?) {
        if (metrics.size >= 3) return
        when (node) {
            is JsonObject -> node.forEach { (childKey, child) -> visit(child, childKey) }
            is JsonArray -> if (key in metricArrayKeys) addMetric(keyMetricLabel(key), node.size.toLong())
            is JsonPrimitive -> {
                val value = node.longOrNull ?: node.contentOrNull?.toLongOrNull()
                when (key) {
                    "pages_crawled", "pages", "page_count", "documents", "docs" -> addMetric("pages", value)
                    "chunks", "chunk_count", "vectors", "points" -> addMetric("chunks", value)
                    "items", "count", "results" -> addMetric("items", value)
                }
            }
        }
    }
    visit(result, null)
    return metrics.distinct().take(3).joinToString(" · ").ifBlank { null }
}

internal val metricArrayKeys = setOf("pages", "documents", "docs", "chunks", "items", "results", "urls")

internal fun keyMetricLabel(key: String?): String = when (key) {
    "urls" -> "items"
    "documents", "docs" -> "pages"
    else -> key ?: "items"
}

@Composable
internal fun statusTone(status: String, activeTone: Color): Color {
    val colors = AxonTheme.colors
    return when (status.lowercase()) {
        "done", "completed", "success" -> colors.success
        "failed", "error", "cancelled", "canceled" -> colors.error
        "pending", "running", "processing" -> activeTone
        else -> colors.textMuted
    }
}

internal fun formatWhen(epochMs: Long): String {
    val ageMs = System.currentTimeMillis() - epochMs
    val minute = 60_000L
    val hour = 60 * minute
    return when {
        ageMs < minute -> "just now"
        ageMs < hour -> "${ageMs / minute}m ago"
        ageMs < 24 * hour -> "${ageMs / hour}h ago"
        else -> SimpleDateFormat("MMM d", Locale.US).format(Date(epochMs))
    }
}
