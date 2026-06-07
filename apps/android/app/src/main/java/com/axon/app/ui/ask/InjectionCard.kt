package com.axon.app.ui.ask

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.Error
import androidx.compose.material.icons.rounded.FilterAlt
import androidx.compose.material.icons.rounded.Pending
import androidx.compose.material.icons.rounded.TravelExplore
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.fab.FabOp
import com.axon.app.ui.common.AuroraProgressBar
import com.axon.app.ui.common.ProgressSize
import com.axon.app.ui.common.ProgressVariant
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.AxonTone
import com.axon.app.ui.theme.tint
import com.axon.app.ui.theme.toneOf

@Composable
fun InjectionCard(
    item: ChatItem.Injection,
    modifier: Modifier = Modifier,
) {
    val op = item.op
    val jobId = item.jobId
    val pageCount = item.pageCount
    val chunkCount = item.chunkCount
    val colors = AxonTheme.colors
    val warm = colors.toneOf(AxonTone.Orange)
    val icon = when (op) {
        FabOp.Crawl -> Icons.Rounded.TravelExplore
        FabOp.Extract -> Icons.Rounded.FilterAlt
        else -> Icons.Rounded.Download
    }
    val shape = RoundedCornerShape(10.dp)

    Column(
        modifier = modifier
            .fillMaxWidth(0.84f)
            .widthIn(max = 356.dp)
            .clip(shape)
            .background(colors.panelStrong.copy(alpha = 0.22f), shape)
            .border(1.dp, colors.tint(warm.base, 9, colors.panelStrong), shape)
            .padding(horizontal = 14.dp, vertical = 13.dp),
        verticalArrangement = Arrangement.spacedBy(11.dp),
    ) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                modifier = Modifier
                    .size(34.dp)
                    .background(colors.tint(warm.base, 8, colors.pageBg), RoundedCornerShape(8.dp))
                    .border(1.dp, colors.tint(warm.base, 15, colors.panelStrong), RoundedCornerShape(8.dp)),
                contentAlignment = Alignment.Center,
            ) {
                Icon(icon, contentDescription = null, tint = warm.fg.copy(alpha = 0.84f), modifier = Modifier.size(16.dp))
            }
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(
                    op.label,
                    color = colors.textPrimary.copy(alpha = 0.90f),
                    fontSize = 14.6.sp,
                    lineHeight = 18.5.sp,
                    fontWeight = FontWeight.ExtraBold,
                    fontFamily = AxonTheme.fonts.display,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    item.endpoint,
                    color = colors.textMuted.copy(alpha = 0.58f),
                    fontSize = 10.4.sp,
                    lineHeight = 13.2.sp,
                    fontFamily = AxonTheme.fonts.mono,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            JobStatusPill(injectionStatusLabel(item))
        }
        Text(
            text = injectionNarrative(item),
            color = colors.textPrimary.copy(alpha = 0.80f),
            fontSize = 12.3.sp,
            lineHeight = 17.2.sp,
            fontFamily = AxonTheme.fonts.body,
        )
        if (!item.isIndexedEvent()) {
            AsyncProgressStrip(item)
        } else {
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(7.dp),
            ) {
                JobMetaPill(item.target)
                if (jobId != null) JobMetaPill("job ${jobId.take(8)}")
            }
        }
    }
}

private fun injectionStatusLabel(item: ChatItem.Injection): String {
    val indexed = item.isIndexedEvent()
    return when {
        indexed && item.op == FabOp.Ingest -> "INGESTED"
        indexed -> "CRAWLED"
        item.status.contains("fail", ignoreCase = true) || item.status.contains("error", ignoreCase = true) -> "FAILED"
        else -> "QUEUED"
    }
}

@Composable
private fun injectionNarrative(item: ChatItem.Injection) = buildAnnotatedString {
    val colors = AxonTheme.colors
    val warm = colors.toneOf(AxonTone.Orange)
    if (!item.isIndexedEvent()) {
        append(item.detail)
        return@buildAnnotatedString
    }
    append("axon mobile just ")
    append(if (item.op == FabOp.Ingest) "ingested " else "crawled ")
    withStyle(SpanStyle(fontFamily = AxonTheme.fonts.mono, color = warm.fg)) {
        append(item.target)
    }
    append(" and indexed ")
    append("%,d".format(item.pageCount ?: 0))
    append(" docs")
    item.chunkCount?.let { chunks ->
        append(" (")
        append("%,d".format(chunks))
        append(" chunks)")
    }
    append(" into your knowledge base — ")
    withStyle(SpanStyle(fontFamily = AxonTheme.fonts.mono, color = colors.textMuted)) {
        append("query · retrieve · ask")
    }
    append(" via MCP or CLI.")
}

private fun ChatItem.Injection.isIndexedEvent(): Boolean =
    pageCount != null && !status.contains("fail", ignoreCase = true) && !status.contains("error", ignoreCase = true)

@Composable
private fun AsyncProgressStrip(item: ChatItem.Injection) {
    val colors = AxonTheme.colors
    val complete = item.status.contains("complete", ignoreCase = true) || item.status.startsWith("2")
    val failed = item.status.contains("fail", ignoreCase = true) || item.status.contains("error", ignoreCase = true)
    val progress = when {
        failed -> 1f
        complete -> 1f
        item.pageCount != null || item.chunkCount != null -> 0.72f
        item.jobId != null -> null
        else -> 0.18f
    }
    val variant = when {
        failed -> ProgressVariant.Error
        complete -> ProgressVariant.Success
        else -> ProgressVariant.Cyan
    }
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        AuroraProgressBar(
            progress = progress,
            variant = variant,
            size = ProgressSize.Sm,
            modifier = Modifier.fillMaxWidth(),
        )
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(
                if (complete) "indexed" else if (failed) "failed" else "running",
                color = if (complete) colors.success.copy(alpha = 0.90f) else if (failed) colors.error.copy(alpha = 0.90f) else colors.textMuted.copy(alpha = 0.62f),
                fontSize = 9.6.sp,
                fontWeight = FontWeight.SemiBold,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
            )
            Text(
                item.jobId?.let { "job ${it.take(8)}" } ?: "waiting for job id",
                color = colors.textMuted.copy(alpha = 0.56f),
                fontSize = 9.6.sp,
                fontFamily = AxonTheme.fonts.mono,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun JobStatusPill(status: String) {
    val colors = AxonTheme.colors
    val isDone = status.contains("complete", ignoreCase = true) || status.startsWith("2")
    val isFailed = status.contains("fail", ignoreCase = true) || status.contains("error", ignoreCase = true)
    val tintColor = when {
        isFailed -> colors.error
        isDone -> colors.success
        else -> colors.warn
    }
    val icon = when {
        isFailed -> Icons.Rounded.Error
        isDone -> Icons.Rounded.CheckCircle
        else -> Icons.Rounded.Pending
    }
    Row(
        modifier = Modifier
            .clip(RoundedCornerShape(999.dp))
            .background(colors.tint(tintColor, 8, colors.panelStrong))
            .border(1.dp, colors.tint(tintColor, 15, colors.panelStrong), RoundedCornerShape(999.dp))
            .padding(horizontal = 8.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Icon(icon, contentDescription = null, tint = tintColor.copy(alpha = 0.82f), modifier = Modifier.size(11.dp))
        Text(
            status,
            color = tintColor.copy(alpha = 0.84f),
            fontSize = 9.8.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.mono,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun JobMetaPill(text: String, accent: Boolean = false) {
    val colors = AxonTheme.colors
    val tone = colors.toneOf(AxonTone.Cyan)
    Text(
        text,
        color = if (accent) tone.fg.copy(alpha = 0.78f) else colors.textMuted.copy(alpha = 0.58f),
        fontSize = 10.sp,
        lineHeight = 13.sp,
        fontFamily = AxonTheme.fonts.mono,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier
            .clip(RoundedCornerShape(999.dp))
            .background(colors.tint(if (accent) tone.base else colors.borderStrong, 7, colors.panelStrong))
            .border(1.dp, colors.tint(if (accent) tone.base else colors.borderStrong, 14, colors.panelStrong), RoundedCornerShape(999.dp))
            .padding(horizontal = 8.dp, vertical = 4.dp),
    )
}
