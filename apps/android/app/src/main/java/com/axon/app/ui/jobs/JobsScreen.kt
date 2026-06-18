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
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.Schedule
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
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
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.repository.JobFamily
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.WatchUi
import com.axon.app.ui.common.rememberRevealState
import com.axon.app.ui.common.revealOnce
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

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
    var selectedJob by remember { mutableStateOf<JobUi?>(null) }
    var crawledPages by remember { mutableStateOf<List<String>>(emptyList()) }
    var crawledPagesLoading by remember { mutableStateOf(false) }
    var crawledPagesError by remember { mutableStateOf<String?>(null) }
    val overviewRows = jobOverviewRows(jobsByKind, watches)
    val reveal = rememberRevealState()

    LaunchedEffect(selectedJob?.id, selectedJob?.resultJson) {
        val job = selectedJob
        crawledPages = emptyList()
        crawledPagesError = null
        crawledPagesLoading = job?.kind == JobFamily.Crawl
        if (job?.kind == JobFamily.Crawl) {
            vm.crawledPagesFor(job).fold(
                onSuccess = { pages -> crawledPages = pages },
                onFailure = { error -> crawledPagesError = error.message ?: "Unable to load crawl manifest" },
            )
            crawledPagesLoading = false
        }
    }

    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        if (selectedJob != null) {
            JobDetailScreen(
                job = selectedJob!!,
                crawledPages = crawledPages,
                crawledPagesLoading = crawledPagesLoading,
                crawledPagesError = crawledPagesError,
                modifier = Modifier
                    .fillMaxWidth(0.96f)
                    .widthIn(max = 460.dp)
                    .padding(top = 16.dp),
                onBack = { selectedJob = null },
            )
        } else {
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
                        itemsIndexed(overviewRows, key = { _, row -> row.key }) { index, row ->
                            JobOverviewRow(
                                row = row,
                                modifier = Modifier
                                    .animateItem()
                                    .revealOnce(reveal, row.key, index),
                                onClick = { drill = row.drill },
                            )
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
                            itemsIndexed(visibleJobs, key = { _, job -> "${selected.kind}-${job.id}" }) { index, job ->
                                JobDrillRow(
                                    job,
                                    modifier = Modifier
                                        .animateItem()
                                        .revealOnce(reveal, "${selected.kind}-${job.id}", index),
                                    onClick = { selectedJob = job },
                                )
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
                            itemsIndexed(watches, key = { _, watch -> watch.id }) { index, watch ->
                                WatchDrillRow(
                                    watch,
                                    modifier = Modifier
                                        .animateItem()
                                        .revealOnce(reveal, watch.id, index),
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

private sealed interface JobDrill {
    data class Kind(val kind: JobFamily) : JobDrill
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
    jobsByKind: Map<JobFamily, List<JobUi>>,
    watches: List<WatchUi>,
): List<JobOverviewRowModel> {
    val colors = AxonTheme.colors
    fun row(kind: JobFamily): JobOverviewRowModel {
        val jobs = jobsByKind[kind].orEmpty()
        val runningCount = jobs.count { isActiveJobStatus(it.status) }
        val failedCount = jobs.count { it.status.lowercase() in setOf("failed", "error") }
        val running = jobs.firstOrNull { isActiveJobStatus(it.status) }
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
            progress = running?.let { progressForJob(it) },
            icon = iconForKind(kind),
            tone = when (kind) {
                JobFamily.Crawl -> colors.accentPrimary
                JobFamily.Embed -> colors.accentPink
                JobFamily.Extract -> colors.orange
                JobFamily.Ingest -> colors.accentStrong
            },
            drill = JobDrill.Kind(kind),
        )
    }
    return listOf(
        row(JobFamily.Crawl),
        row(JobFamily.Embed),
        row(JobFamily.Ingest),
        row(JobFamily.Extract),
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
private fun JobOverviewRow(row: JobOverviewRowModel, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(8.dp)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(colors.control.copy(alpha = 0.06f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.12f), shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 18.dp, vertical = 18.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(15.dp),
    ) {
        Icon(row.icon, contentDescription = null, tint = colors.tint(row.tone, 78, colors.textPrimary), modifier = Modifier.size(22.dp))
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    row.title,
                    color = colors.textPrimary,
                    fontSize = 15.2.sp,
                    lineHeight = 20.sp,
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
                fontSize = 12.4.sp,
                lineHeight = 16.4.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            row.progress?.let { progress ->
                ProgressBar(progress, row.tone, modifier = Modifier.width(156.dp))
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
            fontSize = 11.2.sp,
            lineHeight = 14.sp,
            fontFamily = AxonTheme.fonts.mono,
        )
    }
}

@Composable
private fun FailedCount(count: Int) {
    Text(
        "$count×",
        color = AxonTheme.colors.error,
        fontSize = 11.2.sp,
        lineHeight = 14.sp,
        fontFamily = AxonTheme.fonts.mono,
    )
}
