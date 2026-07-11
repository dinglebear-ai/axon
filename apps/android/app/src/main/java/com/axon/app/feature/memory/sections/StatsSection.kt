package com.axon.app.feature.memory.sections

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.BarChart
import androidx.compose.material.icons.rounded.Hub
import androidx.compose.material.icons.rounded.Schedule
import androidx.compose.material.icons.rounded.Speed
import androidx.compose.material.icons.rounded.Storage
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.axon.app.ui.common.ErrorContent
import com.axon.app.ui.common.LoadingContent
import com.axon.app.ui.common.Resource
import com.axon.app.ui.common.humanLabel
import com.axon.app.ui.common.rememberRevealState
import com.axon.app.ui.common.revealOnce
import com.axon.app.feature.memory.KnowledgeViewModel
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

@Composable
fun StatsSection(vm: KnowledgeViewModel) {
    val state by vm.stats.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { vm.loadStats() }

    when (val s = state) {
        Resource.Idle, Resource.Loading -> LoadingContent(
            label = "Loading stats…",
            modifier = Modifier.fillMaxWidth(),
        )
        is Resource.Error -> ErrorContent(message = s.message, onRetry = { vm.loadStats(force = true) })
        is Resource.Ready -> {
            val model = remember(s.value) { StatsDashboard.from(s.value) }
            val reveal = rememberRevealState()
            // Running index so StatLine rows cascade in across all three sections.
            var lineIndex = 0
            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                item {
                    FlowRow(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        StatTile(Icons.Rounded.Storage, "Collection", model.collection)
                        StatTile(Icons.Rounded.Hub, "Vectors", model.indexedVectors)
                        StatTile(Icons.Rounded.BarChart, "Documents", model.docsEmbedded)
                        StatTile(Icons.Rounded.Schedule, "Freshness", model.lastIndexed)
                    }
                }

                item { StatSectionLabel("Operations") }
                model.counts.forEach {
                    val idx = lineIndex++
                    item(key = "ops-${it.label}") {
                        StatLine(it.label, it.value, it.tone, modifier = Modifier.animateItem().revealOnce(reveal, "ops-${it.label}", idx))
                    }
                }

                item { StatSectionLabel("Performance") }
                model.performance.forEach {
                    val idx = lineIndex++
                    item(key = "perf-${it.label}") {
                        StatLine(it.label, it.value, it.tone, modifier = Modifier.animateItem().revealOnce(reveal, "perf-${it.label}", idx))
                    }
                }

                item { StatSectionLabel("Freshness") }
                model.freshness.forEach {
                    val idx = lineIndex++
                    item(key = "fresh-${it.label}") {
                        StatLine(it.label, it.value, it.tone, modifier = Modifier.animateItem().revealOnce(reveal, "fresh-${it.label}", idx))
                    }
                }

                if (model.payloadFields.isNotEmpty()) {
                    item { StatSectionLabel("Payload Fields") }
                    item {
                        FlowRow(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            model.payloadFields.take(48).forEach { FieldChip(it) }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun StatTile(icon: ImageVector, label: String, value: String) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(13.dp)
    Row(
        modifier = Modifier
            .clip(shape)
            .background(colors.control, shape)
            .border(1.dp, colors.borderDefault, shape)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(9.dp),
    ) {
        Icon(icon, contentDescription = null, tint = colors.accentStrong)
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(label.uppercase(), color = colors.textMuted, fontSize = 9.5.sp, fontFamily = AxonTheme.fonts.mono, maxLines = 1)
            Text(value, color = colors.textPrimary, fontSize = 12.5.sp, fontWeight = FontWeight.SemiBold, fontFamily = AxonTheme.fonts.body, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
private fun StatSectionLabel(text: String) {
    Text(
        text.uppercase(),
        color = AxonTheme.colors.accentStrong,
        fontSize = 10.5.sp,
        fontWeight = FontWeight.Bold,
        fontFamily = AxonTheme.fonts.mono,
        letterSpacing = 1.4.sp,
        modifier = Modifier.padding(start = 2.dp, top = 2.dp),
    )
}

@Composable
private fun StatLine(label: String, value: String, tone: StatTone = StatTone.Neutral, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    val toneColor = when (tone) {
        StatTone.Neutral -> colors.textMuted
        StatTone.Good -> colors.success
        StatTone.Warn -> colors.warn
        StatTone.Error -> colors.error
    }
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(11.dp))
            .background(colors.control.copy(alpha = 0.6f))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.7f), RoundedCornerShape(11.dp))
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(label, color = colors.textPrimary, fontSize = 12.sp, fontFamily = AxonTheme.fonts.body, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(value, color = toneColor, fontSize = 11.sp, fontFamily = AxonTheme.fonts.mono, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun FieldChip(label: String) {
    val colors = AxonTheme.colors
    Text(
        label,
        color = colors.textMuted,
        fontSize = 10.5.sp,
        fontFamily = AxonTheme.fonts.mono,
        modifier = Modifier
            .clip(RoundedCornerShape(999.dp))
            .background(colors.tint(colors.accentPrimary, 7, colors.control))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.55f), RoundedCornerShape(999.dp))
            .padding(horizontal = 9.dp, vertical = 5.dp),
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

private data class StatsDashboard(
    val collection: String,
    val indexedVectors: String,
    val docsEmbedded: String,
    val lastIndexed: String,
    val counts: List<StatDatum>,
    val performance: List<StatDatum>,
    val freshness: List<StatDatum>,
    val payloadFields: List<String>,
) {
    companion object {
        fun from(payload: JsonElement): StatsDashboard {
            val root = payload.jsonObject
            val freshnessObj = root["freshness"] as? JsonObject ?: JsonObject(emptyMap())
            val countsObj = root["counts"] as? JsonObject ?: JsonObject(emptyMap())
            return StatsDashboard(
                collection = root.stringValue("collection") ?: "Unknown",
                indexedVectors = root.longValue("indexed_vectors_count", "vectors", "points_count").countLabel(),
                docsEmbedded = root.longValue("docs_embedded_estimate").countLabel(),
                lastIndexed = freshnessObj.longValue("last_indexed_secs_ago").secondsAgoLabel(),
                counts = countsObj.entries
                    .filter { it.value !is kotlinx.serialization.json.JsonNull }
                    .sortedBy { it.key }
                    .map { StatDatum(humanLabel(it.key), it.value.primitiveLabel()) },
                performance = listOf(
                    StatDatum("Average Crawl Duration", root.doubleValue("avg_crawl_duration_seconds").secondsLabel()),
                    StatDatum("Average Embedding Duration", root.doubleValue("avg_embedding_duration_seconds").secondsLabel()),
                    StatDatum("Average Pages Per Second", root.doubleValue("avg_pages_crawled_per_second").rateLabel("pages/s")),
                    StatDatum("Average Chunks Per Document", root.doubleValue("avg_chunks_per_doc").decimalLabel()),
                ),
                freshness = freshnessObj.entries
                    .filter { it.value !is kotlinx.serialization.json.JsonNull }
                    .sortedBy { it.key }
                    .map { StatDatum(humanLabel(it.key), if (it.key.endsWith("secs_ago")) it.value.longPrimitive().secondsAgoLabel() else it.value.primitiveLabel()) },
                payloadFields = (root["payload_fields"] as? JsonArray)
                    ?.mapNotNull { it.jsonPrimitive.contentOrNull }
                    .orEmpty(),
            )
        }
    }
}

private data class StatDatum(
    val label: String,
    val value: String,
    val tone: StatTone = StatTone.Neutral,
)

private enum class StatTone { Neutral, Good, Warn, Error }

private fun JsonObject.stringValue(key: String): String? =
    this[key]?.jsonPrimitive?.contentOrNull

private fun JsonObject.longValue(vararg keys: String): Long? =
    keys.firstNotNullOfOrNull { key -> this[key]?.longPrimitive() }

private fun JsonObject.doubleValue(key: String): Double? =
    this[key]?.jsonPrimitive?.doubleOrNull

private fun JsonElement.longPrimitive(): Long? =
    (this as? JsonPrimitive)?.longOrNull

private fun JsonElement.primitiveLabel(): String =
    when (this) {
        is JsonPrimitive -> when {
            longOrNull != null -> longOrNull.countLabel()
            doubleOrNull != null -> doubleOrNull.decimalLabel()
            contentOrNull.isNullOrBlank() -> "Not set"
            else -> contentOrNull.orEmpty()
        }
        else -> "Available"
    }

private fun Long?.countLabel(): String =
    this?.let { "%,d".format(it) } ?: "Not set"

private fun Long?.secondsAgoLabel(): String =
    this?.let {
        when {
            it < 60 -> "${it}s ago"
            it < 3_600 -> "${it / 60}m ago"
            it < 86_400 -> "${it / 3_600}h ago"
            else -> "${it / 86_400}d ago"
        }
    } ?: "Not set"

private fun Double?.secondsLabel(): String =
    this?.let { "${"%.1f".format(it)}s" } ?: "Not set"

private fun Double?.rateLabel(suffix: String): String =
    this?.let { "${"%.2f".format(it)} $suffix" } ?: "Not set"

private fun Double?.decimalLabel(): String =
    this?.let { "%.2f".format(it) } ?: "Not set"
