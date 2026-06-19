package com.axon.app.ui.common

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material.icons.rounded.WarningAmber
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

internal enum class NoticeTone { Info, Warn, Error }

@Composable
internal fun AppNoticeBanner(
    message: String,
    modifier: Modifier = Modifier,
    tone: NoticeTone = NoticeTone.Warn,
    icon: ImageVector = if (tone == NoticeTone.Info) Icons.Rounded.Info else Icons.Rounded.WarningAmber,
    maxLines: Int = 4,
) {
    val colors = AxonTheme.colors
    val accent = when (tone) {
        NoticeTone.Info -> colors.accentStrong
        NoticeTone.Warn -> colors.warn
        NoticeTone.Error -> colors.warn
    }
    val border = when (tone) {
        NoticeTone.Info -> colors.tint(accent, 18, colors.pageBg)
        NoticeTone.Warn -> colors.tint(accent, 18, colors.pageBg)
        NoticeTone.Error -> colors.tint(colors.error, 10, colors.pageBg)
    }
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(8.dp))
            .background(colors.tint(accent, 4, colors.pageBg), RoundedCornerShape(8.dp))
            .border(1.dp, border, RoundedCornerShape(8.dp))
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(9.dp),
    ) {
        Icon(
            icon,
            contentDescription = null,
            tint = accent,
            modifier = Modifier
                .size(16.dp)
                .padding(top = 1.dp),
        )
        Text(
            message,
            color = colors.textMuted.copy(alpha = 0.96f),
            fontSize = 12.4.sp,
            lineHeight = 17.sp,
            fontWeight = FontWeight.Medium,
            fontFamily = AxonTheme.fonts.body,
            maxLines = maxLines,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
    }
}
