package com.axon.app.ui.fab

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
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

private val RingRadius = 132.dp
private val TileWidth = 74.dp

@Composable
fun FabRing(
    visible: Boolean,
    fabCenterOffset: IntOffset,
    onOpSelected: (FabOp) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val density = LocalDensity.current

    val openProgress by animateFloatAsState(
        targetValue = if (visible) 1f else 0f,
        animationSpec = spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMedium),
        label = "ring-open",
    )
    if (!visible && openProgress == 0f) return

    Box(modifier = modifier.fillMaxSize()) {
        // Dim backdrop — tap to dismiss.
        if (openProgress > 0f) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(Color(0xFF040A0E).copy(alpha = openProgress * 0.94f))
                    .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
            )
        }

        val radiusPx = with(density) { RingRadius.toPx() }
        val halfTileWPx = with(density) { (TileWidth / 2).roundToPx() }
        val iconHalfPx = with(density) { 22.dp.roundToPx() }
        val halfDismissPx = with(density) { 21.dp.roundToPx() }

        FabRingOps.forEachIndexed { i, op ->
            val angleRad = Math.toRadians(-90.0 + i * (360.0 / FabRingOps.size))
            val dx = (radiusPx * cos(angleRad) * openProgress).roundToInt()
            val dy = (radiusPx * sin(angleRad) * openProgress).roundToInt()

            OpTile(
                op = op,
                // Centre the icon on the ring point; the label hangs below it.
                modifier = Modifier.offset {
                    IntOffset(
                        x = fabCenterOffset.x + dx - halfTileWPx,
                        y = fabCenterOffset.y + dy - iconHalfPx,
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
    Column(
        modifier = modifier
            .width(TileWidth)
            .graphicsLayer { this.alpha = alpha }
            .clip(RoundedCornerShape(12.dp))
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick)
            .padding(vertical = 2.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Box(
            modifier = Modifier
                .size(44.dp)
                .clip(RoundedCornerShape(11.dp))
                .background(colors.tint(tone.base, 16, colors.pageBg), RoundedCornerShape(11.dp))
                .border(1.dp, colors.tint(tone.base, 34, colors.panelStrong), RoundedCornerShape(11.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                imageVector = op.icon,
                contentDescription = null,
                tint = tone.fg,
                modifier = Modifier.size(20.dp),
            )
        }
        Text(
            op.label,
            color = colors.textPrimary.copy(alpha = 0.92f),
            fontSize = 10.5.sp,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center,
        )
    }
}
