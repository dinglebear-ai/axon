package com.axon.app.ui.jobs

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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.data.repository.JobUi
import com.axon.app.data.repository.RecentJob
import com.axon.app.data.repository.WatchUi
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint
import tv.tootie.aurora.components.AuroraProgress
import tv.tootie.aurora.components.AuroraProgressSize
import tv.tootie.aurora.components.AuroraProgressVariant
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone

@Composable
internal fun JobDrillRow(job: JobUi, modifier: Modifier = Modifier) {
    val colors = AxonTheme.colors
    val tone = jobTone(job.kind)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 18.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
            AuroraStatusIndicator(tone = statusIndicatorTone(job.status), dotOnly = true, dotSize = 7.dp)
            Text(
                shortTarget(jobDisplayTarget(job)),
                color = colors.textPrimary,
                fontSize = 12.4.sp,
                lineHeight = 15.8.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Text(
                statusLabel(job.status),
                color = colors.textMuted.copy(alpha = 0.84f),
                fontSize = 10.2.sp,
                lineHeight = 12.8.sp,
                fontFamily = AxonTheme.fonts.mono,
            )
        }
        if (job.status.lowercase() !in setOf("idle")) {
            ProgressBar(progressForJob(job), tone, modifier = Modifier.width(166.dp).padding(start = 14.dp))
        }
        Text(
            jobProgressLabel(job),
            color = colors.textMuted.copy(alpha = 0.78f),
            fontSize = 10.5.sp,
            lineHeight = 13.4.sp,
            fontFamily = AxonTheme.fonts.mono,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(start = 14.dp),
        )
        job.errorText?.takeIf { it.isNotBlank() }?.let { error ->
            Text(
                humanizeJsonFragmentText(error),
                color = colors.error,
                fontSize = 10.5.sp,
                lineHeight = 13.4.sp,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(start = 14.dp),
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
            AuroraStatusIndicator(
                tone = if (watch.enabled) AuroraStatusTone.Online else AuroraStatusTone.Offline,
                dotOnly = true,
                dotSize = 8.dp,
            )
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

        ProgressBar(progressForStatus(job.status), tone)

        Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
            Text(
                job.id.take(12),
                color = colors.textMuted,
                fontSize = 11.sp,
                fontFamily = AxonTheme.fonts.mono,
            )
            Text(
                "${(progressForStatus(job.status) * 100).toInt()}%",
                color = colors.textMuted,
                fontSize = 11.sp,
                fontFamily = AxonTheme.fonts.mono,
            )
        }
    }
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
            AuroraStatusIndicator(tone = AuroraStatusTone.Syncing, dotOnly = true, dotSize = 5.dp)
        }
    }
}

@Composable
internal fun ProgressBar(progress: Float, tone: Color) {
    ProgressBar(progress = progress, tone = tone, modifier = Modifier.fillMaxWidth())
}

@Composable
internal fun ProgressBar(progress: Float, tone: Color, modifier: Modifier) {
    AuroraProgress(
        value = progress.coerceIn(0f, 1f),
        variant = progressVariantForTone(tone),
        size = AuroraProgressSize.Compact,
        modifier = modifier,
        trackColor = AxonTheme.colors.borderDefault.copy(alpha = 0.28f),
    )
}

@Composable
private fun progressVariantForTone(tone: Color): AuroraProgressVariant {
    val colors = AxonTheme.colors
    return when (tone) {
        colors.success -> AuroraProgressVariant.Success
        colors.warn -> AuroraProgressVariant.Warn
        colors.error -> AuroraProgressVariant.Error
        colors.accentPink -> AuroraProgressVariant.Rose
        else -> AuroraProgressVariant.Default
    }
}

private fun statusIndicatorTone(status: String): AuroraStatusTone = when (status.lowercase()) {
    "pending", "queued" -> AuroraStatusTone.Queued
    "running", "processing", "in_progress" -> AuroraStatusTone.Syncing
    "done", "completed", "success", "succeeded" -> AuroraStatusTone.Online
    "failed", "error" -> AuroraStatusTone.Error
    "cancelled", "canceled", "idle" -> AuroraStatusTone.Offline
    else -> AuroraStatusTone.Degraded
}

@Composable
internal fun EmptyJobsCard(title: String, subtitle: String) {
    val colors = AxonTheme.colors
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(colors.control, RoundedCornerShape(13.dp))
            .border(1.dp, colors.borderDefault, RoundedCornerShape(13.dp))
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Text(title, color = colors.textPrimary, fontSize = 14.sp, fontWeight = FontWeight.Bold, fontFamily = AxonTheme.fonts.display)
        Text(subtitle, color = colors.textMuted, fontSize = 12.sp, lineHeight = 17.sp, fontFamily = AxonTheme.fonts.body)
    }
}

@Composable
internal fun JobsErrorCard(message: String) {
    val colors = AxonTheme.colors
    Text(
        humanizeJsonFragmentText(message),
        color = colors.error,
        fontSize = 12.sp,
        fontFamily = AxonTheme.fonts.body,
        modifier = Modifier
            .fillMaxWidth()
            .background(colors.control, RoundedCornerShape(13.dp))
            .border(1.dp, colors.error.copy(alpha = 0.45f), RoundedCornerShape(13.dp))
            .padding(13.dp),
    )
}
