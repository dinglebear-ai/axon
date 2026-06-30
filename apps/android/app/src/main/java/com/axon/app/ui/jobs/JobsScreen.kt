package com.axon.app.ui.jobs

import androidx.activity.compose.BackHandler
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
import androidx.compose.material.icons.automirrored.rounded.PlaylistAdd
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
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.repository.JobFamily
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.WatchUi
import com.axon.app.ui.common.AxonElevation
import com.axon.app.ui.common.AxonBadge
import com.axon.app.ui.common.RecoveryActionCard
import com.axon.app.ui.common.axonElevation
import com.axon.app.ui.common.rememberRevealState
import com.axon.app.ui.common.revealOnce
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

@Composable
fun JobsScreen(
    onOpenAsk: () -> Unit = {},
    onNestedBackAvailableChange: (Boolean) -> Unit = {},
    vm: JobsOverviewViewModel = viewModel(),
) {
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
    var selectedJobRef by remember { mutableStateOf<JobRef?>(null) }
    var crawledPages by remember { mutableStateOf<List<String>>(emptyList()) }
    var crawledPagesLoading by remember { mutableStateOf(false) }
    var crawledPagesError by remember { mutableStateOf<String?>(null) }
    val overviewRows = jobOverviewRows(jobsByKind, watches)
    val hasAnyJobs = jobsByKind.values.any { it.isNotEmpty() } || watches.isNotEmpty()
    val reveal = rememberRevealState()
    val selectedJob = selectedJobRef?.let { ref ->
        jobsByKind[ref.kind].orEmpty().firstOrNull { it.id == ref.id }
    }
    val selectedCrawlManifestPath = remember(selectedJob?.id, selectedJob?.resultJson) {
        crawlManifestArtifactPath(selectedJob?.resultJson)
    }

    LaunchedEffect(selectedJob?.id, selectedCrawlManifestPath) {
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
    val canHandleNestedBack = selectedJob != null || drill != null
    LaunchedEffect(canHandleNestedBack) {
        onNestedBackAvailableChange(canHandleNestedBack)
    }
    DisposableEffect(Unit) {
        onDispose { onNestedBackAvailableChange(false) }
    }
    BackHandler(enabled = canHandleNestedBack) {
        if (selectedJobRef != null) {
            selectedJobRef = null
        } else {
            drill = null
        }
    }

    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        if (selectedJob != null) {
            JobDetailScreen(
                job = selectedJob,
                crawledPages = crawledPages,
                crawledPagesLoading = crawledPagesLoading,
                crawledPagesError = crawledPagesError,
                modifier = Modifier
                    .fillMaxWidth()
                    .widthIn(max = 520.dp)
                    .padding(start = 6.dp, top = 10.dp, end = 6.dp),
                onBack = { selectedJobRef = null },
            )
        } else {
            LazyColumn(
                modifier = Modifier
                    .fillMaxWidth()
                    .widthIn(max = 520.dp)
                    .padding(start = 6.dp, top = 10.dp, end = 6.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                when (val selected = drill) {
                    null -> {
                        item { SectionLabel("Jobs") }
                        if (error != null && active.isEmpty() && jobsByKind.isEmpty()) {
                            item {
                                JobsErrorCard(
                                    message = error.orEmpty(),
                                    onRetry = vm::refresh,
                                )
                            }
                        } else if (!hasAnyJobs) {
                            item {
                                RecoveryActionCard(
                                    title = "No jobs yet",
                                    message = "Start from Ask or the action launcher, then return here to watch crawl, ingest, embed, and extract work progress.",
                                    primaryLabel = "Create a job",
                                    onPrimary = onOpenAsk,
                                    secondaryLabel = "Refresh",
                                    onSecondary = vm::refresh,
                                    icon = Icons.AutoMirrored.Rounded.PlaylistAdd,
                                )
                            }
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
                                HierarchyJobRow(
                                    job = job,
                                    modifier = Modifier
                                        .animateItem()
                                        .revealOnce(reveal, "${selected.kind}-${job.id}", index),
                                    onClick = { selectedJobRef = JobRef(selected.kind, job.id) },
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
                                HierarchyWatchRow(
                                    watch = watch,
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

private data class JobRef(val kind: JobFamily, val id: String)

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
        val activeJobs = jobs.filter { isActiveJobStatus(it.status) }
        val runningCount = activeJobs.size
        val failedCount = jobs.count { it.status.lowercase() in setOf("failed", "error") }
        val running = activeJobs.firstOrNull()
        val representative = running ?: jobs.firstOrNull()
        val aggregateProgress = aggregateProgressForJobs(activeJobs)
        return JobOverviewRowModel(
            key = kind.name,
            title = kind.drillTitle(),
            detail = representative?.let { job ->
                val suffix = when {
                    activeJobs.size > 1 -> "${activeJobs.size} active ${kind.drillTitle().lowercase()} · avg ${((aggregateProgress ?: 0f) * 100).toInt()}%"
                    running != null -> jobProgressLabel(job)
                    job.status.lowercase() in setOf("done", "completed", "success") -> "latest · ${jobProgressLabel(job)}"
                    else -> jobProgressLabel(job)
                }
                "${shortTarget(jobDisplayTarget(job))} · $suffix"
            }
                ?: "No ${kind.label().lowercase()} jobs",
            runningCount = runningCount,
            failedCount = failedCount,
            progress = aggregateProgress,
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
    val quiet = row.runningCount == 0 && row.failedCount == 0 && row.progress == null
    Row(
        modifier = modifier
            .fillMaxWidth()
            .axonElevation(shape, AxonElevation.Row)
            .clip(shape)
            .background(colors.control.copy(alpha = if (quiet) 0.018f else 0.07f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = if (quiet) 0.04f else 0.14f), shape)
            .semantics(mergeDescendants = true) {
                contentDescription = buildString {
                    append(row.title)
                    if (row.detail.isNotBlank()) append(", ").append(row.detail)
                    if (row.runningCount > 0) append(", ").append(row.runningCount).append(" running")
                    if (row.failedCount > 0) append(", ").append(row.failedCount).append(" failed")
                }
                role = Role.Button
            }
            .clickable(onClick = onClick)
            .padding(horizontal = if (quiet) 18.dp else 20.dp, vertical = if (quiet) 13.dp else 20.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(if (quiet) 13.dp else 15.dp),
    ) {
        Icon(row.icon, contentDescription = null, tint = colors.tint(row.tone, 78, colors.textPrimary), modifier = Modifier.size(if (quiet) 22.dp else 24.dp))
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(if (quiet) 4.dp else 9.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    row.title,
                    color = colors.textPrimary,
                    fontSize = if (quiet) 15.sp else 16.sp,
                    lineHeight = if (quiet) 19.5.sp else 21.5.sp,
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
                fontSize = if (quiet) 12.4.sp else 13.2.sp,
                lineHeight = if (quiet) 16.sp else 18.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            row.progress?.let { progress ->
                ProgressBar(progress, row.tone, modifier = Modifier.width(156.dp))
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                "View jobs",
                color = colors.textMuted.copy(alpha = 0.82f),
                fontSize = 11.sp,
                lineHeight = 14.sp,
                fontFamily = AxonTheme.fonts.body,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
            )
            Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = colors.textMuted.copy(alpha = 0.76f), modifier = Modifier.size(18.dp))
        }
    }
}

@Composable
private fun HierarchyJobRow(job: JobUi, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val colors = AxonTheme.colors
    val tone = jobTone(job.kind)
    val statusTone = jobStatusTone(job.status, tone)
    val source = job.sourceType
        ?.replace('_', ' ')
        ?.replaceFirstChar { it.uppercase() }
        ?: job.kind?.label()
        ?: "Job"
    val shape = RoundedCornerShape(8.dp)

    Column(
        modifier = modifier
            .fillMaxWidth()
            .axonElevation(shape, AxonElevation.Row)
            .clip(shape)
            .background(colors.control.copy(alpha = 0.045f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.1f), shape)
            .semantics(mergeDescendants = true) {
                contentDescription = "${job.kind?.label() ?: "Job"} ${job.status}, ${jobDisplayTarget(job)}, view job details"
                role = Role.Button
            }
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(9.dp)) {
            Box(
                modifier = Modifier
                    .size(9.dp)
                    .background(statusTone, RoundedCornerShape(999.dp)),
            )
            Text(
                shortTarget(jobDisplayTarget(job)),
                color = colors.textPrimary,
                fontSize = 13.2.sp,
                lineHeight = 17.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Text(
                "View details",
                color = colors.textMuted.copy(alpha = 0.84f),
                fontSize = 10.8.sp,
                lineHeight = 13.4.sp,
                fontFamily = AxonTheme.fonts.body,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
            )
            Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = colors.textMuted.copy(alpha = 0.66f), modifier = Modifier.size(16.dp))
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            AxonBadge(job.kind?.label() ?: "Job", tone)
            AxonBadge(statusLabel(job.status), statusTone)
            AxonBadge(source, colors.textMuted)
        }
        if (isActiveJobStatus(job.status) || isCompletedJobStatus(job.status)) {
            ProgressBar(progressForJob(job), tone, modifier = Modifier.width(188.dp))
        }
        coverageSummary(job)?.let { summary ->
            AxonBadge(summary, tone)
        }
        Text(
            jobProgressLabel(job),
            color = colors.textMuted.copy(alpha = 0.78f),
            fontSize = 11.4.sp,
            lineHeight = 15.sp,
            fontFamily = AxonTheme.fonts.mono,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        job.errorText?.takeIf { it.isNotBlank() }?.let { error ->
            Text(
                error,
                color = colors.error,
                fontSize = 11.4.sp,
                lineHeight = 15.sp,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun HierarchyWatchRow(watch: WatchUi, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    val tone = if (watch.enabled) colors.accentPrimary else colors.textMuted
    val shape = RoundedCornerShape(8.dp)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .axonElevation(shape, AxonElevation.Row)
            .clip(shape)
            .background(colors.control.copy(alpha = 0.04f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.1f), shape)
            .padding(horizontal = 14.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(9.dp)) {
            Box(
                modifier = Modifier
                    .size(9.dp)
                    .background(tone, RoundedCornerShape(999.dp)),
            )
            Text(
                watch.name,
                color = colors.textPrimary,
                fontSize = 13.sp,
                lineHeight = 17.sp,
                fontFamily = AxonTheme.fonts.body,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            AxonBadge("Watch", colors.accentPrimary)
            AxonBadge(if (watch.enabled) "Enabled" else "Paused", tone)
            AxonBadge(watch.taskType, colors.textMuted)
        }
        Text(
            "Every ${watch.everySeconds}s · next ${watch.nextRunAt ?: "not scheduled"}",
            color = colors.textMuted.copy(alpha = 0.78f),
            fontSize = 11.4.sp,
            lineHeight = 15.sp,
            fontFamily = AxonTheme.fonts.mono,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun jobStatusTone(status: String, activeTone: Color): Color {
    val colors = AxonTheme.colors
    return when (status.lowercase()) {
        in ACTIVE_JOB_STATUSES -> activeTone
        in COMPLETED_JOB_STATUSES -> colors.success
        "failed", "error" -> colors.error
        else -> colors.textMuted
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
