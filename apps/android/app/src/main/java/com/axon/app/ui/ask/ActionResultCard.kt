package com.axon.app.ui.ask

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.Error
import androidx.compose.material.icons.rounded.Pending
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.humanizeJsonFragmentText
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.AxonTone
import com.axon.app.ui.theme.tint
import com.axon.app.ui.theme.toneOf

@Composable
fun ActionResultCard(
    item: ChatItem.ActionResult,
    modifier: Modifier = Modifier,
) {
    val colors = AxonTheme.colors
    val tone = colors.toneOf(if (item.op.isAsync) AxonTone.Orange else AxonTone.Cyan)
    val shape = RoundedCornerShape(10.dp)
    val bodyText = humanizeJsonFragmentText(item.body)

    Column(
        modifier = modifier
            .fillMaxWidth(0.88f)
            .widthIn(max = 390.dp)
            .clip(shape)
            .background(colors.panelStrong.copy(alpha = 0.20f), shape)
            .border(1.dp, colors.borderDefault.copy(alpha = 0.20f), shape)
            .padding(horizontal = 15.dp, vertical = 14.dp),
        verticalArrangement = Arrangement.spacedBy(13.dp),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(11.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(38.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(colors.tint(tone.base, 8, colors.panelStrong))
                    .border(1.dp, colors.tint(tone.base, 18, colors.panelStrong), RoundedCornerShape(8.dp)),
                contentAlignment = Alignment.Center,
            ) {
                Icon(item.op.icon, contentDescription = null, tint = tone.fg.copy(alpha = 0.90f), modifier = Modifier.size(18.dp))
            }
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(
                    item.op.label,
                    color = colors.textPrimary.copy(alpha = 0.92f),
                    fontSize = 16.sp,
                    lineHeight = 20.sp,
                    fontWeight = FontWeight.ExtraBold,
                    fontFamily = AxonTheme.fonts.display,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    item.endpoint,
                    color = colors.textMuted.copy(alpha = 0.58f),
                    fontSize = 11.sp,
                    lineHeight = 14.sp,
                    fontFamily = AxonTheme.fonts.mono,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            ResultStatusPill(item.status)
        }

        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            ResultMetaPill(item.target)
            ResultMetaPill(item.summary, accent = true)
        }

        Text(
            bodyText,
            color = colors.textPrimary.copy(alpha = 0.84f),
            fontSize = 13.sp,
            lineHeight = 18.6.sp,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 10,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun ResultStatusPill(status: String) {
    val colors = AxonTheme.colors
    val kind = resultStatusKind(status)
    val tintColor = when (kind) {
        ResultStatusKind.Success -> colors.success
        ResultStatusKind.Warning -> colors.warn
        ResultStatusKind.Error -> colors.error
    }
    val icon = when (kind) {
        ResultStatusKind.Success -> Icons.Rounded.CheckCircle
        ResultStatusKind.Warning -> Icons.Rounded.Pending
        ResultStatusKind.Error -> Icons.Rounded.Error
    }
    Row(
        modifier = Modifier
            .clip(RoundedCornerShape(999.dp))
            .background(colors.tint(tintColor, 8, colors.panelStrong))
            .border(1.dp, colors.tint(tintColor, 18, colors.panelStrong), RoundedCornerShape(999.dp))
            .padding(horizontal = 9.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Icon(icon, contentDescription = null, tint = tintColor.copy(alpha = 0.86f), modifier = Modifier.size(12.dp))
        Text(status, color = tintColor.copy(alpha = 0.90f), fontSize = 10.6.sp, fontWeight = FontWeight.SemiBold, fontFamily = AxonTheme.fonts.mono)
    }
}

internal enum class ResultStatusKind { Success, Warning, Error }

internal fun resultStatusKind(status: String): ResultStatusKind {
    val normalized = status.trim().lowercase()
    val code = normalized.takeWhile { it.isDigit() }.toIntOrNull()
    return when {
        code == 202 -> ResultStatusKind.Warning
        code != null && code in 200..299 -> ResultStatusKind.Success
        code != null && code >= 400 -> ResultStatusKind.Error
        normalized.contains("fail") || normalized.contains("error") || normalized.contains("unavailable") -> ResultStatusKind.Error
        normalized.contains("queued") || normalized.contains("running") || normalized.contains("pending") || normalized.startsWith("202") -> ResultStatusKind.Warning
        else -> ResultStatusKind.Success
    }
}

@Composable
private fun ResultMetaPill(text: String, accent: Boolean = false) {
    val colors = AxonTheme.colors
    val tone = colors.toneOf(AxonTone.Cyan)
    Text(
        text,
        color = if (accent) tone.fg.copy(alpha = 0.78f) else colors.textMuted.copy(alpha = 0.58f),
        fontSize = 11.sp,
        lineHeight = 14.sp,
        fontFamily = AxonTheme.fonts.mono,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier
            .clip(RoundedCornerShape(999.dp))
            .background(colors.tint(if (accent) tone.base else colors.borderStrong, 7, colors.panelStrong))
            .border(1.dp, colors.tint(if (accent) tone.base else colors.borderStrong, 14, colors.panelStrong), RoundedCornerShape(999.dp))
            .padding(horizontal = 9.dp, vertical = 5.dp),
    )
}
