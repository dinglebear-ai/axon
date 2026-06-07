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
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
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
                    .background(Color(0xFF040A0E).copy(alpha = openProgress * 0.94f))
                    .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
            )
        }

        val radiusPx = with(density) { radiusDp.toPx() }
        val halfTilePx = with(density) { 22.dp.roundToPx() }
        val halfDismissPx = with(density) { 21.dp.roundToPx() }
        FabRingOps.forEachIndexed { i, op ->
            val angleRad = Math.toRadians(-90.0 + i * (360.0 / FabRingOps.size))
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

        Box(
            modifier = Modifier
                .offset {
                    IntOffset(
                        x = fabCenterOffset.x - halfDismissPx,
                        y = fabCenterOffset.y - halfDismissPx,
                    )
                }
                .size(42.dp)
                .graphicsLayer { this.alpha = openProgress }
                .background(AxonTheme.colors.panelMedium.copy(alpha = 0.32f), RoundedCornerShape(12.dp))
                .border(1.dp, AxonTheme.colors.borderStrong.copy(alpha = 0.48f), RoundedCornerShape(12.dp))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Rounded.Close,
                contentDescription = "Close operations",
                tint = AxonTheme.colors.textPrimary.copy(alpha = 0.78f),
                modifier = Modifier.size(16.dp),
            )
        }
    }
}

@Composable
private fun OpTile(op: FabOp, modifier: Modifier, alpha: Float, onClick: () -> Unit) {
    val colors = AxonTheme.colors
    val tone = colors.toneOf(if (op.isAsync) AxonTone.Orange else AxonTone.Cyan)
    val bg = colors.panelStrong
    val iconBg = colors.tint(tone.base, 13, colors.pageBg)
    val iconBorder = colors.tint(tone.base, 30, colors.panelStrong)
    val border = if (op.isAsync) colors.tint(tone.base, 36, colors.panelStrong) else colors.borderStrong

    Box(
        modifier = modifier
            .size(44.dp)
            .graphicsLayer { this.alpha = alpha }
            .clip(RoundedCornerShape(10.dp))
            .background(bg.copy(alpha = 0.09f), RoundedCornerShape(10.dp))
            .border(1.dp, border.copy(alpha = 0.32f), RoundedCornerShape(10.dp))
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            modifier = Modifier
                .size(32.dp)
                .background(iconBg.copy(alpha = 0.62f), RoundedCornerShape(8.dp))
                .border(1.dp, iconBorder.copy(alpha = 0.60f), RoundedCornerShape(8.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                imageVector = op.icon,
                contentDescription = op.label,
                tint = tone.fg.copy(alpha = 0.82f),
                modifier = Modifier.size(16.dp),
            )
        }
    }
}
