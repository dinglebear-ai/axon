package com.axon.app.ui.common

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.tint

@Composable
internal fun CommandConsoleBackground(
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    val colors = AxonTheme.colors
    Box(
        modifier = modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    listOf(
                        colors.tint(colors.accentPrimary, 5, colors.pageBg),
                        colors.pageBg,
                        colors.tint(colors.accentPink, 3, colors.pageBg),
                    ),
                ),
            ),
    ) {
        SignalRail(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 18.dp, vertical = 16.dp),
            tint = colors.borderDefault.copy(alpha = 0.18f),
        )
        content()
    }
}

@Composable
internal fun SignalRail(
    modifier: Modifier = Modifier,
    tint: Color = AxonTheme.colors.borderDefault.copy(alpha = 0.2f),
) {
    Canvas(modifier = modifier) {
        val step = 42.dp.toPx()
        var y = 0f
        while (y < size.height) {
            drawLine(tint, Offset(0f, y), Offset(size.width, y), strokeWidth = 1f)
            y += step
        }
        val accent = tint.copy(alpha = (tint.alpha * 1.7f).coerceAtMost(0.5f))
        drawLine(accent, Offset(size.width * 0.18f, 0f), Offset(size.width * 0.18f, size.height), strokeWidth = 1.2f)
        drawLine(accent, Offset(size.width * 0.82f, 0f), Offset(size.width * 0.82f, size.height), strokeWidth = 1.2f)
    }
}

@Composable
internal fun CommandConsoleHeader(
    eyebrow: String,
    title: String,
    description: String,
    modifier: Modifier = Modifier,
    tone: Color = AxonTheme.colors.accentPrimary,
    icon: ImageVector? = null,
    metrics: @Composable RowScope.() -> Unit = {},
) {
    val colors = AxonTheme.colors
    val shape = RoundedCornerShape(14.dp)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .axonElevation(shape, AxonElevation.Card)
            .clip(shape)
            .background(
                Brush.verticalGradient(
                    listOf(
                        colors.tint(tone, 12, colors.panelStrong).copy(alpha = 0.96f),
                        colors.tint(colors.accentPink, 4, colors.panelMedium).copy(alpha = 0.9f),
                    ),
                ),
                shape,
            )
            .border(1.dp, colors.tint(tone, 28, colors.panelStrong), shape)
            .padding(horizontal = 14.dp, vertical = 13.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(34.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(colors.tint(tone, 22, colors.control))
                    .border(1.dp, colors.tint(tone, 42, colors.control), RoundedCornerShape(10.dp)),
                contentAlignment = Alignment.Center,
            ) {
                if (icon != null) {
                    Icon(icon, contentDescription = null, tint = colors.tint(tone, 88, colors.textPrimary), modifier = Modifier.size(18.dp))
                } else {
                    SignalGlyph(tone = tone)
                }
            }
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    eyebrow.uppercase(),
                    color = colors.tint(tone, 72, colors.textPrimary),
                    fontSize = 10.sp,
                    lineHeight = 12.sp,
                    fontWeight = FontWeight.ExtraBold,
                    fontFamily = AxonTheme.fonts.mono,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    title,
                    color = colors.textPrimary,
                    fontSize = 18.sp,
                    lineHeight = 22.sp,
                    fontWeight = FontWeight.ExtraBold,
                    fontFamily = AxonTheme.fonts.display,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Text(
            description,
            color = colors.textMuted.copy(alpha = 0.86f),
            fontSize = 12.4.sp,
            lineHeight = 17.sp,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(7.dp),
            verticalAlignment = Alignment.CenterVertically,
            content = metrics,
        )
    }
}

@Composable
internal fun MetricPill(
    label: String,
    value: String,
    modifier: Modifier = Modifier,
    tone: Color = AxonTheme.colors.accentPrimary,
) {
    val colors = AxonTheme.colors
    Row(
        modifier = modifier
            .height(28.dp)
            .clip(RoundedCornerShape(999.dp))
            .background(colors.tint(tone, 10, colors.control))
            .border(1.dp, colors.tint(tone, 24, colors.control), RoundedCornerShape(999.dp))
            .padding(horizontal = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Box(
            modifier = Modifier
                .size(6.dp)
                .clip(RoundedCornerShape(999.dp))
                .background(colors.tint(tone, 90, colors.textPrimary)),
        )
        Text(
            label,
            color = colors.textMuted.copy(alpha = 0.82f),
            fontSize = 9.5.sp,
            lineHeight = 12.sp,
            fontWeight = FontWeight.SemiBold,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
        )
        Text(
            value,
            color = colors.textPrimary.copy(alpha = 0.94f),
            fontSize = 10.sp,
            lineHeight = 12.sp,
            fontWeight = FontWeight.ExtraBold,
            fontFamily = AxonTheme.fonts.mono,
            maxLines = 1,
        )
    }
}

@Composable
internal fun MetricFlow(
    modifier: Modifier = Modifier,
    content: @Composable RowScope.() -> Unit,
) {
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(7.dp),
        verticalAlignment = Alignment.CenterVertically,
        content = content,
    )
}

@Composable
private fun SignalGlyph(tone: Color) {
    val colors = AxonTheme.colors
    Canvas(modifier = Modifier.size(18.dp)) {
        val center = Offset(size.width / 2f, size.height / 2f)
        val arm = size.minDimension * 0.36f
        val stroke = size.minDimension * 0.09f
        drawLine(colors.borderStrong, Offset(center.x - arm, center.y), Offset(center.x + arm, center.y), stroke)
        drawLine(colors.borderStrong, Offset(center.x, center.y - arm), Offset(center.x, center.y + arm), stroke)
        drawCircle(colors.tint(tone, 90, colors.textPrimary), size.minDimension * 0.16f, center)
        drawCircle(colors.tint(tone, 52, colors.panelStrong), size.minDimension * 0.31f, center, style = androidx.compose.ui.graphics.drawscope.Stroke(width = stroke))
    }
}
