package com.axon.app.ui.jobs

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.CloudDownload
import androidx.compose.material.icons.rounded.DataObject
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Schedule
import androidx.compose.material.icons.rounded.TravelExplore
import androidx.compose.material.icons.rounded.Work
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.RecentJob
import com.axon.app.data.repository.WatchUi
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.longOrNull
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Composable
fun JobsScreen(vm: JobsOverviewViewModel = viewModel()) {
    DisposableEffect(vm) {
        vm.setVisible(true)
        onDispose { vm.setVisible(false) }
    }
    val active by vm.activeJobs.collectAsStateWithLifecycle()
    val jobsByKind by vm.jobsByKind.collectAsStateWithLifecycle()
    val recent by vm.recentJobs.collectAsStateWithLifecycle()
    val watches by vm.watches.collectAsStateWithLifecycle()
    val error by vm.errorMessage.collectAsStateWithLifecycle()
    var drill by remember { mutableStateOf<JobDrill?>(null) }
    val overviewRows = jobOverviewRows(jobsByKind, watches)

    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        LazyColumn(
            modifier = Modifier
                .fillMaxWidth(0.96f)
                .widthIn(max = 460.dp)
                .padding(top = 16.dp),
            verticalArrangement = Arrangement.spacedBy(13.dp),
        ) {
            when (val selected = drill) {
                null -> {
                    item { SectionLabel("Jobs") }
                    if (error != null && active.isEmpty() && jobsByKind.isEmpty()) {
                        item { JobsErrorCard(error.orEmpty()) }
                    }
                    items(overviewRows, key = { it.key }) { row ->
                        JobOverviewRow(row = row, onClick = { drill = row.drill })
                    }
                }
                is JobDrill.Kind -> {
                    val jobs = jobsByKind[selected.kind].orEmpty()
                    val visibleJobs = jobs.take(25)
                    item {
                        DrillHeader(
                            title = selected.kind.drillTitle(),
                            detail = if (jobs.size > visibleJobs.size) "${visibleJobs.size}/${jobs.size}" else "${jobs.size}",
                            onBack = { drill = null },
                        )
                    }
                    if (jobs.isEmpty()) {
                        item { EmptyJobsCard("No ${selected.kind.label().lowercase()} jobs", "New ${selected.kind.label().lowercase()} submissions appear here.") }
                    } else {
                        items(visibleJobs, key = { "${selected.kind}-${it.id}" }) { job ->
                            JobDrillRow(job)
                        }
                        if (jobs.size > visibleJobs.size) {
                            item {
                                MoreJobsHint(remaining = jobs.size - visibleJobs.size)
                            }
                        }
                    }
                }
                JobDrill.Watches -> {
                    item {
                        DrillHeader(
                            title = "Watches",
                            detail = "${watches.size} ${if (watches.size == 1) "job" else "jobs"}",
                            onBack = { drill = null },
                        )
                    }
                    if (watches.isEmpty()) {
                        item { EmptyJobsCard("No watches", "Recurring URL change detectors appear here.") }
                    } else {
                        items(watches, key = { it.id }) { watch ->
                            WatchDrillRow(watch)
                        }
                    }
                }
            }
        }
    }
}

private sealed interface JobDrill {
    data class Kind(val kind: AxonClient.JobKind) : JobDrill
    data object Watches : JobDrill
}

private data class JobOverviewRowModel(
    val key: String,
    val title: String,
    val detail: String,
    val runningCount: Int,
    val failedCount: Int,
    val progress: Float?,
    val icon: ImageVector,
    val tone: Color,
    val drill: JobDrill,
)

@Composable
private fun jobOverviewRows(
    jobsByKind: Map<AxonClient.JobKind, List<JobUi>>,
    watches: List<WatchUi>,
): List<JobOverviewRowModel> {
    val colors = AxonTheme.colors
    fun row(kind: AxonClient.JobKind): JobOverviewRowModel {
        val jobs = jobsByKind[kind].orEmpty()
        val runningCount = jobs.count { it.status.lowercase() in setOf("pending", "running", "processing") }
        val failedCount = jobs.count { it.status.lowercase() in setOf("failed", "error") }
        val running = jobs.firstOrNull { it.status.lowercase() in setOf("pending", "running", "processing") }
        val representative = running ?: jobs.firstOrNull()
        return JobOverviewRowModel(
            key = kind.name,
            title = kind.drillTitle(),
            detail = representative?.let { job ->
                val suffix = if (running == null && job.status.lowercase() in setOf("done", "completed", "success")) {
                    "latest · ${jobProgressLabel(job)}"
                } else {
                    jobProgressLabel(job)
                }
                "${shortTarget(jobDisplayTarget(job))} · $suffix"
            }
                ?: "No ${kind.label().lowercase()} jobs",
            runningCount = runningCount,
            failedCount = failedCount,
            progress = representative
                ?.takeIf { it.status.lowercase() !in setOf("idle", "pending") || running != null }
                ?.let { progressForJob(it) },
            icon = iconForKind(kind),
            tone = when (kind) {
                AxonClient.JobKind.Crawl -> colors.accentPrimary
                AxonClient.JobKind.Embed -> colors.accentPink
                AxonClient.JobKind.Extract -> colors.orange
                AxonClient.JobKind.Ingest -> colors.accentStrong
            },
            drill = JobDrill.Kind(kind),
        )
    }
    return listOf(
        row(AxonClient.JobKind.Crawl),
        row(AxonClient.JobKind.Embed),
        row(AxonClient.JobKind.Ingest),
        row(AxonClient.JobKind.Extract),
        JobOverviewRowModel(
            key = "watches",
            title = "Watches",
            detail = "${watches.size} ${if (watches.size == 1) "job" else "jobs"} · ${if (watches.any { it.enabled }) "enabled" else "idle"}",
            runningCount = watches.count { it.enabled },
            failedCount = 0,
            progress = null,
            icon = Icons.Rounded.Schedule,
            tone = AxonTheme.colors.accentPrimary,
            drill = JobDrill.Watches,
        ),
    )
}

@Composable
private fun JobOverviewRow(row: JobOverviewRowModel, onClick: () -> Unit) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(8.dp)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(colors.control.copy(alpha = 0.06f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.12f), shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 17.dp, vertical = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Icon(row.icon, contentDescription = null, tint = colors.tint(row.tone, 78, colors.textPrimary), modifier = Modifier.size(20.dp))
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    row.title,
                    color = colors.textPrimary,
                    fontSize = 14.2.sp,
                    lineHeight = 18.8.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = AxonTheme.fonts.body,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                if (row.runningCount > 0) StatusCount(row.runningCount, row.tone)
                if (row.failedCount > 0) FailedCount(row.failedCount)
            }
            Text(
                row.detail,
                color = colors.textMuted.copy(alpha = 0.82f),
                fontSize = 11.5.sp,
                lineHeight = 15.2.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            row.progress?.let { progress ->
                ProgressBar(progress, row.tone, modifier = Modifier.width(132.dp))
            }
        }
        Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = colors.textMuted.copy(alpha = 0.76f), modifier = Modifier.size(18.dp))
    }
}

@Composable
private fun StatusCount(count: Int, tone: Color) {
    val colors = AxonTheme.colors
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
        Box(
            modifier = Modifier
                .size(5.dp)
                .background(if (count > 0) tone else colors.textMuted.copy(alpha = 0.45f), RoundedCornerShape(999.dp)),
        )
        Text(
            count.toString(),
            color = if (count > 0) colors.tint(tone, 75, colors.textPrimary) else colors.textMuted,
            fontSize = 10.2.sp,
            lineHeight = 12.8.sp,
            fontFamily = AxonTheme.fonts.mono,
        )
    }
}

@Composable
private fun FailedCount(count: Int) {
    Text(
        "$count×",
        color = AxonTheme.colors.error,
        fontSize = 10.2.sp,
        lineHeight = 12.8.sp,
        fontFamily = AxonTheme.fonts.mono,
    )
}

@Composable
private fun DrillHeader(title: String, detail: String, onBack: () -> Unit) {
    val colors = AxonTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(50.dp)
            .clip(RoundedCornerShape(9.dp))
            .background(colors.control.copy(alpha = 0.04f), RoundedCornerShape(9.dp))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.08f), RoundedCornerShape(9.dp))
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        Icon(
            Icons.AutoMirrored.Rounded.ArrowBack,
            contentDescription = "Back",
            tint = colors.textMuted,
            modifier = Modifier
                .size(26.dp)
                .clickable(onClick = onBack)
                .padding(6.dp),
        )
        Text(
            title,
            color = colors.textPrimary,
            fontSize = 13.sp,
            lineHeight = 17.4.sp,
            fontWeight = FontWeight.Bold,
            fontFamily = AxonTheme.fonts.display,
            modifier = Modifier.weight(1f),
        )
        Text(detail, color = colors.textMuted.copy(alpha = 0.76f), fontSize = 10.9.sp, lineHeight = 13.8.sp, fontFamily = AxonTheme.fonts.mono)
    }
}
