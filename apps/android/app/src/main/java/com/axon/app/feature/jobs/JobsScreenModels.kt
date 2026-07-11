package com.axon.app.feature.jobs

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Schedule
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import com.axon.app.data.repository.JobFamily
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.WatchUi
import com.axon.app.ui.theme.AxonTheme

/** Drill-down target selected from the [JobsScreen] overview list. */
internal sealed interface JobDrill {
    data class Kind(val kind: JobFamily) : JobDrill
    data object Watches : JobDrill
}

internal data class JobOverviewRowModel(
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
internal fun jobOverviewRows(
    jobsByKind: Map<JobFamily, List<JobUi>>,
    watches: List<WatchUi>,
): List<JobOverviewRowModel> {
    val colors = AxonTheme.colors
    fun row(kind: JobFamily): JobOverviewRowModel {
        val jobs = jobsByKind[kind].orEmpty()
        val activeJobs = jobs.filter { isActiveJobStatus(it.status) }
        val runningCount = activeJobs.size
        val failedCount = jobs.count { isFailedJobStatus(it.status) }
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
