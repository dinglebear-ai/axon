package com.axon.app.feature.jobs

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
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
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.RecentJob
import com.axon.app.data.repository.WatchUi
import com.axon.app.ui.common.AppNoticeBanner
import com.axon.app.ui.common.NoticeTone
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

@Composable
internal fun JobDrillRow(job: JobUi, modifier: Modifier = Modifier, onClick: (() -> Unit)? = null) {
    val colors = AxonTheme.colors
    val tone = jobTone(job.kind)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 18.dp, vertical = 11.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(Modifier.size(8.dp).background(statusTone(job.status, tone), RoundedCornerShape(999.dp)))
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
                statusLabel(job.status),
                color = colors.textMuted.copy(alpha = 0.84f),
                fontSize = 11.sp,
                lineHeight = 14.sp,
                fontFamily = AxonTheme.fonts.mono,
            )
        }
        if (isActiveJobStatus(job.status) || isCompletedJobStatus(job.status)) {
            ProgressBar(progressForJob(job), tone, modifier = Modifier.width(184.dp).padding(start = 16.dp))
        }
        coverageSummary(job)?.let { summary ->
            CoverageChip(summary, tone, modifier = Modifier.padding(start = 16.dp))
        }
        Text(
            jobProgressLabel(job),
            color = colors.textMuted.copy(alpha = 0.78f),
            fontSize = 11.4.sp,
            lineHeight = 15.sp,
            fontFamily = AxonTheme.fonts.mono,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(start = 16.dp),
        )
        job.errorText?.takeIf { it.isNotBlank() }?.let { error ->
            Text(
                humanizeJsonFragmentText(error),
                color = colors.error,
                fontSize = 11.4.sp,
                lineHeight = 15.sp,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(start = 16.dp),
            )
        }
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(1.dp)
                .background(colors.borderDefault.copy(alpha = 0.08f)),
        )
    }
}

@Composable
internal fun MoreJobsHint(remaining: Int) {
    val colors = AxonTheme.colors
    Text(
        "+$remaining more jobs",
        color = colors.textMuted.copy(alpha = 0.78f),
        fontSize = 10.6.sp,
        lineHeight = 13.4.sp,
        fontFamily = AxonTheme.fonts.mono,
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(colors.control.copy(alpha = 0.04f), RoundedCornerShape(8.dp))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.08f), RoundedCornerShape(8.dp))
            .padding(horizontal = 14.dp, vertical = 12.dp),
    )
}

@Composable
internal fun WatchDrillRow(watch: WatchUi, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    val tone = if (watch.enabled) colors.accentPrimary else colors.textMuted
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(colors.control, RoundedCornerShape(12.dp))
            .border(1.dp, colors.borderDefault, RoundedCornerShape(12.dp))
            .padding(horizontal = 14.dp, vertical = 13.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(Modifier.size(8.dp).background(tone, RoundedCornerShape(999.dp)))
            Text(
                watch.name,
                color = colors.textPrimary,
                fontSize = 13.sp,
                fontFamily = AxonTheme.fonts.body,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Text(if (watch.enabled) "enabled" else "paused", color = tone, fontSize = 11.sp, fontFamily = AxonTheme.fonts.mono)
        }
        Text(
            "${watch.taskType} · every ${watch.everySeconds}s · next ${watch.nextRunAt ?: "not scheduled"}",
            color = colors.textMuted,
            fontSize = 11.sp,
            fontFamily = AxonTheme.fonts.mono,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
internal fun RefreshChip(onClick: () -> Unit) {
    val colors = AxonTheme.colors
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(34.dp)
            .clip(RoundedCornerShape(999.dp))
            .background(colors.control, RoundedCornerShape(999.dp))
            .border(1.dp, colors.borderDefault, RoundedCornerShape(999.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(9.dp),
    ) {
        Icon(Icons.Rounded.Refresh, contentDescription = null, tint = colors.textMuted, modifier = Modifier.size(13.dp))
        Text(
            "Pull to refresh · updated moments ago",
            color = colors.textMuted,
            fontSize = 10.3.sp,
            lineHeight = 12.4.sp,
            fontFamily = AxonTheme.fonts.mono,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
internal fun SectionLabel(text: String) {
    Text(
        text.uppercase(),
        color = AxonTheme.colors.accentStrong,
        fontSize = 10.sp,
        fontWeight = FontWeight.Bold,
        fontFamily = AxonTheme.fonts.mono,
        letterSpacing = 1.5.sp,
        modifier = Modifier.padding(top = 3.dp, start = 1.dp),
    )
}

@Composable
internal fun ActiveJobCard(job: JobUi) {
    val colors = AxonTheme.colors
    val tone = jobTone(job.kind)
    val shape = RoundedCornerShape(8.dp)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(colors.control, shape)
            .border(1.dp, colors.borderDefault, shape)
            .padding(13.dp),
        verticalArrangement = Arrangement.spacedBy(11.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            JobIconTile(iconForKind(job.kind), tone)
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    jobDisplayTarget(job),
                    color = colors.textPrimary,
                    fontSize = 13.5.sp,
                    fontFamily = AxonTheme.fonts.mono,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    "${job.kind?.label() ?: "Job"} · ${job.status}",
                    color = colors.textMuted,
                    fontSize = 12.sp,
                    fontFamily = AxonTheme.fonts.body,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            RunningDots(tone)
        }

        ProgressBar(progressForJob(job), tone)
        coverageSummary(job)?.let { summary ->
            CoverageChip(summary, tone)
        }

        Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
            Text(
                job.id.take(12),
                color = colors.textMuted,
                fontSize = 11.sp,
                fontFamily = AxonTheme.fonts.mono,
            )
            Text(
                pagesCrawledMetric(job) ?: "${(progressForJob(job) * 100).toInt()}%",
                color = colors.textMuted,
                fontSize = 11.sp,
                fontFamily = AxonTheme.fonts.mono,
            )
        }
    }
}

@Composable
internal fun CoverageChip(text: String, tone: Color, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    Text(
        text,
        color = colors.tint(tone, 86, colors.textPrimary),
        fontSize = 10.6.sp,
        lineHeight = 13.sp,
        fontFamily = AxonTheme.fonts.body,
        fontWeight = FontWeight.SemiBold,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = modifier
            .clip(RoundedCornerShape(999.dp))
            .background(colors.tint(tone, 13, colors.control), RoundedCornerShape(999.dp))
            .border(1.dp, colors.tint(tone, 28, colors.control), RoundedCornerShape(999.dp))
            .padding(horizontal = 9.dp, vertical = 4.dp),
    )
}

@Composable
internal fun RecentRunRow(job: RecentJob) {
    val colors = AxonTheme.colors
    val tone = toneForKindName(job.kind)
    val shape = RoundedCornerShape(13.dp)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(colors.control.copy(alpha = 0.14f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.24f), shape)
            .padding(horizontal = 12.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(iconForKindName(job.kind), contentDescription = null, tint = colors.tint(tone, 80, colors.textPrimary), modifier = Modifier.size(16.dp))
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(
                job.target,
                color = colors.textPrimary,
                fontSize = 11.sp,
                lineHeight = 13.2.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                "${job.kind.replaceFirstChar { it.uppercase() }} · ${formatWhen(job.submittedAt)}",
                color = colors.textMuted,
                fontSize = 10.sp,
                lineHeight = 12.sp,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Text(
            "done",
            color = colors.success,
            fontSize = 10.sp,
            lineHeight = 12.sp,
            fontFamily = AxonTheme.fonts.mono,
        )
    }
}

@Composable
internal fun JobIconTile(icon: ImageVector, tone: Color, size: Int = 38) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(11.dp)
    Box(
        modifier = Modifier
            .size(size.dp)
            .background(colors.tint(tone, 14, colors.control), shape)
            .border(1.dp, colors.tint(tone, 30, colors.control), shape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null, tint = colors.tint(tone, 80, colors.textPrimary), modifier = Modifier.size((size / 2).dp))
    }
}

@Composable
internal fun RunningDots(tone: Color) {
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
        repeat(3) {
            Box(
                modifier = Modifier
                    .size(5.dp)
                    .background(tone, RoundedCornerShape(999.dp)),
            )
        }
    }
}

@Composable
internal fun ProgressBar(progress: Float, tone: Color) {
    ProgressBar(progress = progress, tone = tone, modifier = Modifier.fillMaxWidth())
}

@Composable
internal fun ProgressBar(progress: Float, tone: Color, modifier: Modifier) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(999.dp)
    Box(
        modifier = modifier
            .height(3.dp)
            .background(colors.borderDefault.copy(alpha = 0.28f), shape)
            .padding(0.7.dp),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth(progress.coerceIn(0.02f, 1f))
                .height(2.dp)
                .clip(shape)
                .background(Brush.horizontalGradient(listOf(AxonTheme.colors.tint(tone, 55, colors.pageBg), tone))),
        )
    }
}

@Composable
internal fun EmptyJobsCard(title: String, subtitle: String) {
    val colors = AxonTheme.colors
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(colors.control.copy(alpha = 0.025f), RoundedCornerShape(9.dp))
            .border(1.dp, colors.borderDefault.copy(alpha = 0.07f), RoundedCornerShape(9.dp))
            .padding(horizontal = 14.dp, vertical = 11.dp),
        verticalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Text(title, color = colors.textPrimary, fontSize = 14.sp, fontWeight = FontWeight.Bold, fontFamily = AxonTheme.fonts.display)
        Text(subtitle, color = colors.textMuted, fontSize = 12.sp, lineHeight = 17.sp, fontFamily = AxonTheme.fonts.body)
    }
}

@Composable
internal fun JobsErrorCard(message: String, onRetry: () -> Unit) {
    com.axon.app.ui.common.RecoveryActionCard(
        title = "Jobs are unavailable",
        message = humanizeJsonFragmentText(message),
        primaryLabel = "Retry",
        onPrimary = onRetry,
        modifier = Modifier.fillMaxWidth(),
    )
}
