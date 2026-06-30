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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.PlaylistAdd
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.History
import androidx.compose.material.icons.rounded.Refresh
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.data.repository.JobFamily
import com.axon.app.data.repository.JobUi
import com.axon.app.ui.common.AxonElevation
import com.axon.app.ui.common.RecoveryActionCard
import com.axon.app.ui.common.axonElevation
import com.axon.app.ui.common.rememberRevealState
import com.axon.app.ui.common.revealOnce
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

@Composable
fun ActivityHistoryScreen(
    onOpenAsk: () -> Unit = {},
    vm: JobsOverviewViewModel = viewModel(),
) {
    DisposableEffect(vm) {
        vm.setVisible(true)
        onDispose { vm.setVisible(false) }
    }

    val recent by vm.recentJobs.collectAsStateWithLifecycle()
    val jobsByKind by vm.jobsByKind.collectAsStateWithLifecycle()
    val error by vm.errorMessage.collectAsStateWithLifecycle()
    val rows = remember(recent, jobsByKind) { recentActivityRows(recent, jobsByKind) }
    var selectedJob by remember { mutableStateOf<JobUi?>(null) }
    var crawledPages by remember { mutableStateOf<List<String>>(emptyList()) }
    var crawledPagesLoading by remember { mutableStateOf(false) }
    var crawledPagesError by remember { mutableStateOf<String?>(null) }
    val selectedCrawlManifestPath = remember(selectedJob?.id, selectedJob?.resultJson) {
        crawlManifestArtifactPath(selectedJob?.resultJson)
    }
    val reveal = rememberRevealState()

    LaunchedEffect(selectedJob?.id, selectedCrawlManifestPath) {
        val job = selectedJob
        crawledPages = emptyList()
        crawledPagesError = null
        crawledPagesLoading = job?.kind == JobFamily.Crawl
        if (job?.kind == JobFamily.Crawl) {
            vm.crawledPagesFor(job).fold(
                onSuccess = { pages -> crawledPages = pages },
                onFailure = { cause -> crawledPagesError = cause.message ?: "Unable to load crawl manifest" },
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
                    .fillMaxWidth()
                    .widthIn(max = 520.dp)
                    .padding(start = 6.dp, top = 10.dp, end = 6.dp),
                onBack = { selectedJob = null },
            )
        } else {
            LazyColumn(
                modifier = Modifier
                    .fillMaxWidth()
                    .widthIn(max = 520.dp)
                    .padding(start = 6.dp, top = 10.dp, end = 6.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                item { SectionLabel("Recent activity") }
                if (error != null && rows.isEmpty()) {
                    item {
                        JobsErrorCard(
                            message = error.orEmpty(),
                            onRetry = vm::refresh,
                        )
                    }
                }
                if (rows.isEmpty()) {
                    item {
                        RecoveryActionCard(
                            title = "No activity yet",
                            message = "Submit an Ask action, crawl, ingest, embed, or extract job. Recent work will appear here with its latest known status.",
                            primaryLabel = "Create activity",
                            onPrimary = onOpenAsk,
                            secondaryLabel = "Refresh",
                            onSecondary = vm::refresh,
                            icon = Icons.AutoMirrored.Rounded.PlaylistAdd,
                        )
                    }
                } else {
                    item {
                        ActivitySummaryCard(
                            rows = rows,
                            onRefresh = vm::refresh,
                        )
                    }
                    itemsIndexed(rows, key = { _, row -> "${row.kind}-${row.recent.jobId}-${row.recent.submittedAt}" }) { index, row ->
                        ActivityHistoryRow(
                            row = row,
                            modifier = Modifier
                                .animateItem()
                                .revealOnce(reveal, "activity-${row.recent.jobId}", index),
                            onClick = { selectedJob = row.job },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ActivitySummaryCard(rows: List<ActivityJobRow>, onRefresh: () -> Unit) {
    val colors = AxonTheme.colors
    val running = rows.count { isActiveJobStatus(it.job.status) }
    val failed = rows.count { it.job.status.lowercase() in setOf("failed", "error", "cancelled", "canceled") }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .axonElevation(RoundedCornerShape(12.dp), AxonElevation.Card)
            .clip(RoundedCornerShape(12.dp))
            .background(colors.panelStrong.copy(alpha = 0.64f), RoundedCornerShape(12.dp))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.26f), RoundedCornerShape(12.dp))
            .clickable(onClick = onRefresh)
            .padding(14.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Rounded.History, contentDescription = null, tint = colors.accentStrong, modifier = Modifier.size(22.dp))
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                "${rows.size} recent ${if (rows.size == 1) "item" else "items"}",
                color = colors.textPrimary,
                fontSize = 15.sp,
                lineHeight = 19.sp,
                fontWeight = FontWeight.SemiBold,
                fontFamily = AxonTheme.fonts.body,
            )
            Text(
                "$running active · $failed need attention · tap to refresh",
                color = colors.textMuted,
                fontSize = 11.5.sp,
                lineHeight = 15.sp,
                fontFamily = AxonTheme.fonts.body,
            )
        }
        Icon(Icons.Rounded.Refresh, contentDescription = null, tint = colors.textMuted, modifier = Modifier.size(17.dp))
    }
}

@Composable
private fun ActivityHistoryRow(row: ActivityJobRow, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val colors = AxonTheme.colors
    val tone = jobTone(row.kind)
    val statusTone = statusTone(row.job.status, tone)
    val shape = RoundedCornerShape(13.dp)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(colors.control.copy(alpha = 0.18f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.22f), shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 13.dp, vertical = 11.dp),
        horizontalArrangement = Arrangement.spacedBy(11.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(9.dp)
                .background(statusTone, RoundedCornerShape(999.dp)),
        )
        Icon(iconForKind(row.kind), contentDescription = null, tint = colors.tint(tone, 80, colors.textPrimary), modifier = Modifier.size(18.dp))
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                shortTarget(jobDisplayTarget(row.job)),
                color = colors.textPrimary,
                fontSize = 13.sp,
                lineHeight = 16.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                "${row.kind.label()} · ${statusLabel(row.job.status)} · ${formatWhen(row.recent.submittedAt)}${if (row.live) "" else " · last seen locally"}",
                color = colors.textMuted,
                fontSize = 11.sp,
                lineHeight = 14.sp,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (isActiveJobStatus(row.job.status) || isCompletedJobStatus(row.job.status)) {
                ProgressBar(progressForJob(row.job), tone, modifier = Modifier.fillMaxWidth(0.82f))
            }
            row.job.errorText?.takeIf { it.isNotBlank() }?.let { error ->
                Text(
                    error,
                    color = colors.error,
                    fontSize = 11.sp,
                    lineHeight = 14.sp,
                    fontFamily = AxonTheme.fonts.body,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Icon(Icons.Rounded.ChevronRight, contentDescription = null, tint = colors.textMuted, modifier = Modifier.size(18.dp))
    }
}
