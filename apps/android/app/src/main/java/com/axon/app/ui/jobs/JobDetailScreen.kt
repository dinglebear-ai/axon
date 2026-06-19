package com.axon.app.ui.jobs

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.data.repository.JobFamily
import com.axon.app.data.repository.JobUi
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.theme.AxonTheme
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

private const val CRAWL_PAGE_PREVIEW_LIMIT = 200

@Composable
internal fun JobDetailScreen(
    job: JobUi,
    crawledPages: List<String>,
    crawledPagesLoading: Boolean,
    crawledPagesError: String?,
    modifier: Modifier = Modifier,
    onBack: () -> Unit,
) {
    Column(
        modifier = modifier
            .verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(13.dp),
    ) {
        DrillHeader(
            title = job.kind?.label()?.let { "$it job" } ?: "Job",
            detail = statusLabel(job.status),
            onBack = onBack,
        )
        JobDetailHero(job)
        JobDetailSection(
            title = "Target",
            rows = listOfNotNull(
                "Target" to jobDisplayTarget(job),
                detailRow("Source", job.sourceType),
                "Job ID" to job.id,
            ),
        )
        JobDetailSection(
            title = "Timing",
            rows = listOfNotNull(
                detailRow("Created", job.createdAt),
                detailRow("Started", job.startedAt),
                detailRow("Updated", job.updatedAt),
                detailRow("Finished", job.finishedAt),
            ),
        )
        job.errorText?.takeIf { it.isNotBlank() }?.let { error ->
            JobDetailSection(title = "Error", rows = listOf("Message" to humanizeJsonFragmentText(error)))
        }
        job.resultJson?.let { result ->
            JobDetailSection(title = "Result", rows = jsonPreviewRows(result))
        }
        if (job.kind == JobFamily.Crawl) {
            CrawlPagesSection(
                pages = crawledPages,
                loading = crawledPagesLoading,
                error = crawledPagesError,
            )
        }
        job.configJson?.let { config ->
            JobDetailSection(title = "Config", rows = jsonPreviewRows(config))
        }
    }
}

@Composable
private fun CrawlPagesSection(
    pages: List<String>,
    loading: Boolean,
    error: String?,
) {
    val rows = when {
        loading -> listOf("Status" to "Loading crawled pages...")
        error != null -> listOf("Error" to error)
        pages.isEmpty() -> listOf("Pages" to "No page list is available for this crawl yet.")
        else -> crawlPageRows(pages)
    }
    JobDetailSection(
        title = if (pages.isEmpty()) "Pages Crawled" else "Pages Crawled (${pages.size})",
        rows = rows,
    )
}

private fun crawlPageRows(pages: List<String>): List<Pair<String, String>> {
    val pageRows = pages
        .take(CRAWL_PAGE_PREVIEW_LIMIT)
        .mapIndexed { index, url -> "#${index + 1}" to url }

    if (pages.size <= CRAWL_PAGE_PREVIEW_LIMIT) return pageRows

    return pageRows + (
        "More" to "Showing first $CRAWL_PAGE_PREVIEW_LIMIT of ${pages.size} crawled pages."
    )
}

@Composable
private fun JobDetailHero(job: JobUi) {
    val colors = AxonTheme.colors
    val tone = jobTone(job.kind)
    val shape = RoundedCornerShape(8.dp)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(colors.control.copy(alpha = 0.08f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.14f), shape)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(13.dp)) {
            JobIconTile(iconForKind(job.kind), tone, size = 42)
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(
                    shortTarget(jobDisplayTarget(job)),
                    color = colors.textPrimary,
                    fontSize = 15.sp,
                    lineHeight = 19.sp,
                    fontWeight = FontWeight.Bold,
                    fontFamily = AxonTheme.fonts.body,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    jobProgressLabel(job),
                    color = colors.textMuted,
                    fontSize = 11.5.sp,
                    lineHeight = 15.sp,
                    fontFamily = AxonTheme.fonts.mono,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (shouldShowJobDetailProgress(job.status)) {
            ProgressBar(progressForJobDetail(job), tone)
        }
        coverageSummary(job)?.let { summary ->
            CoverageChip(summary, tone)
        }
    }
}

@Composable
private fun JobDetailSection(title: String, rows: List<Pair<String, String>>) {
    if (rows.isEmpty()) return
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(8.dp)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(colors.control.copy(alpha = 0.05f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.10f), shape)
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(9.dp),
    ) {
        Text(
            title.uppercase(),
            color = colors.accentStrong,
            fontSize = 10.sp,
            lineHeight = 12.sp,
            fontWeight = FontWeight.Bold,
            fontFamily = AxonTheme.fonts.mono,
            letterSpacing = 1.2.sp,
        )
        rows.forEach { (label, value) ->
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(label, color = colors.textMuted, fontSize = 10.5.sp, lineHeight = 13.sp, fontFamily = AxonTheme.fonts.body)
                Text(
                    value,
                    color = colors.textPrimary.copy(alpha = 0.92f),
                    fontSize = 11.2.sp,
                    lineHeight = 14.6.sp,
                    fontFamily = AxonTheme.fonts.mono,
                    maxLines = 5,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

private fun jsonPreviewRows(element: JsonElement): List<Pair<String, String>> {
    val obj = element as? JsonObject
    if (obj == null) return listOf("Value" to humanizeJsonFragmentText(element.toString()))

    val preferred = listOf(
        "pages_crawled",
        "docs_embedded",
        "chunks_embedded",
        "docs_failed",
        "collection",
        "seed_url",
        "input",
        "url",
        "target",
        "source_type",
    )
    val rows = preferred.mapNotNull { key ->
        obj[key]?.let { value -> key.humanKey() to value.previewValue() }
    }.toMutableList()

    obj.entries
        .asSequence()
        .filter { (key, _) -> key !in preferred }
        .take(6 - rows.size.coerceAtMost(6))
        .forEach { (key, value) -> rows += key.humanKey() to value.previewValue() }

    return rows.take(6).ifEmpty { listOf("Value" to humanizeJsonFragmentText(element.toString())) }
}

private fun detailRow(label: String, value: String?): Pair<String, String>? =
    value?.takeIf { it.isNotBlank() }?.let { label to it }

private fun String.humanKey(): String =
    split('_', '-')
        .filter { it.isNotBlank() }
        .joinToString(" ") { word -> word.replaceFirstChar { it.uppercase() } }

private fun JsonElement.previewValue(): String =
    when (this) {
        is JsonPrimitive -> contentOrNull ?: toString()
        is JsonArray -> "${size} items"
        is JsonObject -> humanizeJsonFragmentText(toString())
        else -> toString()
    }
