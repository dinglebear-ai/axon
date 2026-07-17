package com.axon.app.feature.jobs

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.PlaylistAdd
import androidx.compose.material.icons.rounded.TaskAlt
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.repository.JobFamily
import com.axon.app.ui.common.CommandConsoleHeader
import com.axon.app.ui.common.MetricPill
import com.axon.app.ui.common.RecoveryActionCard
import com.axon.app.ui.common.rememberRevealState
import com.axon.app.ui.common.revealOnce
import com.axon.app.ui.theme.AxonTheme

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
    val selectedJob =
        selectedJobRef?.let { ref ->
            jobsByKind[ref.kind].orEmpty().firstOrNull { it.id == ref.id }
        }
    val selectedCrawlManifestPath =
        remember(selectedJob?.id, selectedJob?.resultJson) {
            crawlManifestArtifactPath(selectedJob?.resultJson)
        }

    LaunchedEffect(selectedJob?.id, selectedCrawlManifestPath) {
        val job = selectedJob
        crawledPages = emptyList()
        crawledPagesError = null
        crawledPagesLoading = job?.kind == JobFamily.Source
        if (job?.kind == JobFamily.Source) {
            vm.crawledPagesFor(job).fold(
                onSuccess = { pages -> crawledPages = pages },
                onFailure = { error -> crawledPagesError = error.message ?: "Unable to load site page manifest" },
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
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .widthIn(max = 520.dp)
                        .padding(start = 6.dp, top = 10.dp, end = 6.dp),
                onBack = { selectedJobRef = null },
            )
        } else {
            LazyColumn(
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .widthIn(max = 520.dp)
                        .padding(start = 6.dp, top = 10.dp, end = 6.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                when (val selected = drill) {
                    null -> {
                        item {
                            CommandConsoleHeader(
                                eyebrow = "operations",
                                title = "Job Command Deck",
                                description = "Track source, extract, and watch activity without losing scan density.",
                                icon = Icons.Rounded.TaskAlt,
                                tone = AxonTheme.colors.accentPrimary,
                            ) {
                                MetricPill("active", active.size.toString())
                                MetricPill(
                                    "families",
                                    overviewRows.count { it.runningCount > 0 || it.failedCount > 0 }.toString(),
                                    tone = AxonTheme.colors.accentPink,
                                )
                                MetricPill("watches", watches.size.toString(), tone = AxonTheme.colors.orange)
                            }
                        }
                        if (error != null) {
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
                                    message = "Start from Ask or the action launcher, then return here to watch source and extract work progress.",
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
                                modifier =
                                    Modifier
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
                            item {
                                EmptyJobsCard(
                                    "No ${selected.kind.label().lowercase()} jobs",
                                    "New ${selected.kind.label().lowercase()} submissions appear here.",
                                )
                            }
                        } else {
                            itemsIndexed(visibleJobs, key = { _, job -> "${selected.kind}-${job.id}" }) { index, job ->
                                HierarchyJobRow(
                                    job = job,
                                    modifier =
                                        Modifier
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
                                    modifier =
                                        Modifier
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

internal data class JobRef(
    val kind: JobFamily,
    val id: String,
)
