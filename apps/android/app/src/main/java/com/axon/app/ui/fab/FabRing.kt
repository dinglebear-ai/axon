package com.axon.app.ui.fab

import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.AxonTone
import com.axon.app.ui.theme.tint
import com.axon.app.ui.theme.toneOf
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

@Composable
fun FabRing(
    visible: Boolean,
    fabCenterOffset: IntOffset,
    onOpSelected: (FabOp) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val radiusDp: Dp = AxonTheme.dimens.opRingRadius
    val density = LocalDensity.current

    val openProgress by animateFloatAsState(
        targetValue = if (visible) 1f else 0f,
        animationSpec = spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMedium),
        label = "ring-open",
    )

    if (!visible && openProgress == 0f) return

    Box(modifier = modifier.fillMaxSize()) {
        // Dim backdrop — only drawn when ring has opened far enough to be meaningful
        if (openProgress > 0f) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(Color(0xFF040A0E).copy(alpha = openProgress * 0.82f))
                    .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
            )
        }

        val radiusPx = with(density) { radiusDp.toPx() }
        val halfTilePx = with(density) { 28.dp.roundToPx() }
        val halfDismissPx = with(density) { 26.dp.roundToPx() }

        FabOp.entries.forEachIndexed { i, op ->
            val angleRad = Math.toRadians(-90.0 + i * 36.0)
            val dx = (radiusPx * cos(angleRad) * openProgress).roundToInt()
            val dy = (radiusPx * sin(angleRad) * openProgress).roundToInt()

            OpTile(
                op = op,
                modifier = Modifier.offset {
                    IntOffset(
                        x = fabCenterOffset.x + dx - halfTilePx,
                        y = fabCenterOffset.y + dy - halfTilePx,
                    )
                },
                alpha = openProgress,
                onClick = { onOpSelected(op) },
            )
        }

        // Center dismiss button
        Box(
            modifier = Modifier
                .offset { IntOffset(fabCenterOffset.x - halfDismissPx, fabCenterOffset.y - halfDismissPx) }
                .size(52.dp)
                .background(AxonTheme.colors.panelMedium, RoundedCornerShape(17.dp))
                .border(1.dp, AxonTheme.colors.borderStrong, RoundedCornerShape(17.dp))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Rounded.Close, contentDescription = "Close", tint = AxonTheme.colors.textPrimary, modifier = Modifier.size(20.dp))
        }
    }
}

@Composable
private fun OpTile(op: FabOp, modifier: Modifier, alpha: Float, onClick: () -> Unit) {
    val colors = AxonTheme.colors
    val tone = colors.toneOf(if (op.isAsync) AxonTone.Orange else AxonTone.Cyan)
    val bg = colors.panelStrong
    val iconBg = colors.tint(tone.base, 14, colors.pageBg)
    val iconBorder = colors.tint(tone.base, 28, colors.panelStrong)
    val border = if (op.isAsync) colors.tint(tone.base, 35, colors.panelStrong) else colors.borderStrong

    Box(
        modifier = modifier
            .size(56.dp)
            .graphicsLayer { this.alpha = alpha }
            .background(bg, RoundedCornerShape(16.dp))
            .border(1.dp, border, RoundedCornerShape(16.dp))
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Box(
                modifier = Modifier
                    .size(28.dp)
                    .background(iconBg, RoundedCornerShape(9.dp))
                    .border(1.dp, iconBorder, RoundedCornerShape(9.dp)),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    imageVector = op.icon,
                    contentDescription = op.label,
                    tint = tone.fg,
                    modifier = Modifier.size(15.dp),
                )
            }
            Text(
                op.label.uppercase(),
                fontSize = 8.sp,
                fontWeight = FontWeight.Bold,
                color = colors.textMuted,
                letterSpacing = 0.4.sp,
                fontFamily = AxonTheme.fonts.body,
                maxLines = 1,
            )
        }
    }
}
