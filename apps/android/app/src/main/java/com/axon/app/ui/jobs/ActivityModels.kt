package com.axon.app.ui.jobs

import com.axon.app.data.repository.JobFamily
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.RecentJob

internal data class ActivityJobRow(
    val recent: RecentJob,
    val kind: JobFamily,
    val job: JobUi,
    val live: Boolean,
)

internal fun recentActivityRows(
    recent: List<RecentJob>,
    jobsByKind: Map<JobFamily, List<JobUi>>,
): List<ActivityJobRow> =
    recent.mapNotNull { item ->
        val kind = jobFamilyFromRecentKind(item.kind) ?: return@mapNotNull null
        val liveJob = jobsByKind[kind].orEmpty().firstOrNull { it.id == item.jobId }
        ActivityJobRow(
            recent = item,
            kind = kind,
            job = liveJob ?: item.toSubmittedFallbackJob(kind),
            live = liveJob != null,
        )
    }

internal fun jobFamilyFromRecentKind(kind: String): JobFamily? =
    JobFamily.entries.firstOrNull { it.name.equals(kind, ignoreCase = true) }

private fun RecentJob.toSubmittedFallbackJob(kind: JobFamily): JobUi =
    JobUi(
        kind = kind,
        id = jobId,
        status = "local-only",
        url = target,
        sourceType = null,
        target = target,
        errorText = "Latest server status unavailable",
        resultJson = null,
    )
