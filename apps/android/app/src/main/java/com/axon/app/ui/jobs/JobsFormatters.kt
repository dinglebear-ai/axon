package com.axon.app.ui.jobs

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.CloudDownload
import androidx.compose.material.icons.rounded.DataObject
import androidx.compose.material.icons.rounded.TravelExplore
import androidx.compose.material.icons.rounded.Work
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import com.axon.app.data.repository.JobFamily
import com.axon.app.data.repository.JobUi
import com.axon.app.ui.theme.AxonTheme
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

internal val ACTIVE_JOB_STATUSES = setOf("pending", "queued", "running", "processing", "in_progress")
internal val COMPLETED_JOB_STATUSES = setOf("done", "completed", "success", "succeeded")
internal val FAILED_JOB_STATUSES = setOf("failed", "error", "cancelled", "canceled")

internal fun isActiveJobStatus(status: String): Boolean =
    status.lowercase() in ACTIVE_JOB_STATUSES

internal fun isCompletedJobStatus(status: String): Boolean =
    status.lowercase() in COMPLETED_JOB_STATUSES

internal fun isFailedJobStatus(status: String): Boolean =
    status.lowercase() in FAILED_JOB_STATUSES

internal fun shouldShowJobDetailProgress(status: String): Boolean =
    isActiveJobStatus(status) || isCompletedJobStatus(status)

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
    if (isCompletedJobStatus(job.status)) return 1f
    val fromProgress = lifecycleProgressFromProgress(job.progressJson)
    return fromProgress
        ?: lifecycleProgressFromCounters(job.progressJson)
        ?: progressForStatus(job.status)
}

internal fun progressForJobDetail(job: JobUi): Float =
    if (isCompletedJobStatus(job.status)) 1f else progressForJob(job)

internal fun aggregateProgressForJobs(jobs: List<JobUi>): Float? {
    var count = 0
    var sum = 0f
    for (job in jobs) {
        if (!isActiveJobStatus(job.status)) continue
        sum += progressForJob(job)
        count++
    }
    if (count == 0) return null
    return (sum / count).coerceIn(0.02f, 1f)
}

internal fun crawledPageUrlsFromResult(result: JsonElement?): List<String> {
    val obj = result as? JsonObject ?: return emptyList()
    val urls = linkedSetOf<String>()
    fun visit(key: String?, element: JsonElement) {
        when (element) {
            is JsonArray -> {
                if (key in crawlPageArrayKeys) {
                    element.forEach { child -> pageUrlFromElement(child)?.let(urls::add) }
                }
            }
            is JsonObject -> element.forEach { (childKey, child) -> visit(childKey, child) }
            is JsonPrimitive -> Unit
        }
    }
    visit(null, obj)
    return urls.toList()
}

internal fun crawlManifestArtifactPath(result: JsonElement?): String? {
    val obj = result as? JsonObject ?: return null
    crawlManifestPathFromHandles(obj)?.let { return it }
    val rawPath = firstString(obj, "manifest_path", "manifest", "output_dir", "worker_output_dir", "output_path", "worker_output_path")
        ?: return null
    return normalizeArtifactManifestPath(rawPath)
}

internal fun parseCrawlManifestUrls(manifestJsonl: String): List<String> =
    manifestJsonl
        .lineSequence()
        .mapNotNull { line ->
            val trimmed = line.trim()
            if (trimmed.isBlank()) return@mapNotNull null
            runCatching {
                Json.parseToJsonElement(trimmed)
                    .jsonObject["url"]
                    ?.jsonPrimitive
                    ?.contentOrNull
                    ?.takeIf { it.isHttpUrl() }
            }.getOrNull()
        }
        .distinct()
        .toList()

internal fun lifecycleProgressFromProgress(progress: JsonElement?): Float? {
    val obj = progress as? JsonObject ?: return null
    val value = obj["lifecycle_progress"]
        ?.let { primitiveFloat(it) }
        ?: obj["progress"]
            ?.let { primitiveFloat(it) }
        ?: return null
    return if (value <= 0f) 0f else value.coerceIn(0.02f, 1f)
}

internal fun lifecycleProgressFromCounters(progress: JsonElement?): Float? {
    val obj = progress as? JsonObject ?: return null
    ratioMetric(obj, "pages_crawled", "pages_discovered")?.let { return it }
    ratioMetric(obj, "docs_embedded", "docs_total")?.let { return it }
    ratioMetric(obj, "docs_completed", "docs_total")?.let { return it }
    ratioMetric(obj, "files_done", "files_total")?.let { return it }
    ratioMetric(obj, "videos_done", "videos_total")?.let { return it }
    ratioMetric(obj, "tasks_done", "tasks_total")?.let { return it }
    return null
}

internal fun coverageSummary(job: JobUi): String? {
    val result = job.resultJson as? JsonObject ?: return null
    val summary = topLevelString(result, "coverage_summary")
    val rawStatus = topLevelString(result, "coverage_status")?.lowercase()
    val rawReason = topLevelString(result, "coverage_reason")?.lowercase()
    val errors = topLevelMetric(result, "error_pages", "errors") ?: 0L
    return when {
        rawReason in MAX_PAGE_LIMIT_REASONS -> "max pages hit"
        rawStatus in COMPLETE_COVERAGE_STATUSES -> {
            if (errors > 0) "complete · $errors errors" else summary ?: "complete"
        }
        rawStatus == "partial" -> if (errors > 0) "partial · $errors errors" else "partial"
        rawStatus == "failed" -> "failed"
        errors > 0 -> "$errors errors"
        else -> summary
    }
}

internal fun pagesCrawledMetric(job: JobUi): String? {
    val value = (job.progressJson as? JsonObject)?.let {
        topLevelMetric(it, "pages_crawled", "pages_seen", "pages_processed")
    } ?: (job.resultJson as? JsonObject)?.let {
        topLevelMetric(it, "pages_crawled", "pages_seen", "pages_processed", "md_created")
    } ?: return null
    return "%,d %s".format(value, if (value == 1L) "page" else "pages")
}

internal fun primitiveLong(element: JsonElement): Long? =
    (element as? JsonPrimitive)?.longOrNull

internal fun primitiveFloat(element: JsonElement): Float? =
    (element as? JsonPrimitive)?.contentOrNull?.toFloatOrNull()

private fun topLevelMetric(obj: JsonObject, vararg keys: String): Long? {
    for (key in keys) {
        val value = obj[key]?.let { primitiveLong(it) }
        if (value != null) return value
    }
    return null
}

private fun ratioMetric(obj: JsonObject, doneKey: String, totalKey: String): Float? {
    val done = topLevelMetric(obj, doneKey) ?: return null
    val total = topLevelMetric(obj, totalKey) ?: return null
    if (total <= 0L) return null
    if (done <= 0L) return 0f
    return (done.toFloat() / total.toFloat()).coerceIn(0.02f, 0.98f)
}

private val crawlPageArrayKeys = setOf(
    "urls",
    "pages",
    "page_urls",
    "crawled_urls",
    "crawled_pages",
    "visited_urls",
    "visited_pages",
    "documents",
    "events",
    "diagnostics",
)

private val MAX_PAGE_LIMIT_REASONS = setOf("max_pages_limit", "max_pages", "page_limit")
private val COMPLETE_COVERAGE_STATUSES = setOf("complete", "completed", "complete_or_exhausted", "exhausted")

private fun pageUrlFromElement(element: JsonElement): String? =
    when (element) {
        is JsonPrimitive -> element.contentOrNull?.takeIf { it.isHttpUrl() }
        is JsonObject -> firstString(element, "url", "href", "source_url")?.takeIf { it.isHttpUrl() }
        else -> null
    }

private fun crawlManifestPathFromHandles(obj: JsonObject): String? {
    val handles = obj["predicted_artifact_handles"] as? JsonArray ?: obj["artifact_handles"] as? JsonArray ?: return null
    return handles
        .mapNotNull { handle ->
            val handleObj = handle as? JsonObject ?: return@mapNotNull null
            firstString(handleObj, "relative_path", "path")?.takeIf { it.endsWith("manifest.jsonl") }
        }
        .firstOrNull()
        ?.let(::normalizeArtifactManifestPath)
}

private fun firstString(obj: JsonObject, vararg keys: String): String? {
    for (key in keys) {
        val value = obj[key]
        if (value is JsonPrimitive) {
            val content = value.contentOrNull?.takeIf { it.isNotBlank() }
            if (content != null) return content
        }
    }
    for ((_, child) in obj) {
        val nested = child as? JsonObject ?: continue
        val value = firstString(nested, *keys)
        if (value != null) return value
    }
    return null
}

private fun topLevelString(obj: JsonObject, vararg keys: String): String? {
    for (key in keys) {
        val value = obj[key]
        if (value is JsonPrimitive) {
            val content = value.contentOrNull?.takeIf { it.isNotBlank() }
            if (content != null) return content
        }
    }
    return null
}

private fun normalizeArtifactManifestPath(rawPath: String): String? {
    val normalized = rawPath.replace('\\', '/').trim().trimEnd('/')
    val manifestPath = when {
        normalized.endsWith("manifest.jsonl") -> normalized
        normalized.endsWith("/markdown") -> normalized.removeSuffix("/markdown") + "/manifest.jsonl"
        else -> "$normalized/manifest.jsonl"
    }
    val relative = when {
        "/output/" in manifestPath -> manifestPath.substringAfterLast("/output/")
        ".axon/output/" in manifestPath -> manifestPath.substringAfterLast(".axon/output/")
        manifestPath.startsWith("/") -> return null
        else -> manifestPath
    }
    return relative
        .takeIf { it.isNotBlank() && !it.contains("..") && it.endsWith("manifest.jsonl") }
}

private fun String.isHttpUrl(): Boolean =
    startsWith("https://", ignoreCase = true) || startsWith("http://", ignoreCase = true)

@Composable
internal fun jobTone(kind: JobFamily?): Color = when (kind) {
    JobFamily.Crawl -> AxonTheme.colors.accentPrimary
    JobFamily.Embed -> AxonTheme.colors.accentPink
    JobFamily.Extract -> AxonTheme.colors.orange
    JobFamily.Ingest -> AxonTheme.colors.accentStrong
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

internal fun iconForKind(kind: JobFamily?): ImageVector = when (kind) {
    JobFamily.Crawl -> Icons.Rounded.TravelExplore
    JobFamily.Embed -> Icons.Rounded.Work
    JobFamily.Extract -> Icons.Rounded.DataObject
    JobFamily.Ingest -> Icons.Rounded.CloudDownload
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
        pagesCrawledMetric(job) ?: resultMetricSummary(job.resultJson) ?: fallbackJobDetail(job),
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
