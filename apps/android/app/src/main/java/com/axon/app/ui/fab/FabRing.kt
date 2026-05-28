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
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

private val BorderStrong = Color(0xFF24536C)
private val PanelStrong  = Color(0xFF13293A)

@Composable
fun FabRing(
    visible: Boolean,
    fabCenterOffset: IntOffset,
    onOpSelected: (FabOp) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val radiusDp: Dp = 96.dp
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

        FabOp.entries.forEachIndexed { i, op ->
            val angleDeg = -90.0 + i * 36.0
            val angleRad = Math.toRadians(angleDeg)
            val radiusPx = with(density) { radiusDp.toPx() }

            val dx = (radiusPx * cos(angleRad) * openProgress).roundToInt()
            val dy = (radiusPx * sin(angleRad) * openProgress).roundToInt()

            OpTile(
                op = op,
                modifier = Modifier.offset {
                    IntOffset(
                        x = fabCenterOffset.x + dx - with(density) { 23.dp.roundToPx() },
                        y = fabCenterOffset.y + dy - with(density) { 23.dp.roundToPx() },
                    )
                },
                alpha = openProgress,
                onClick = { onOpSelected(op) },
            )
        }

        // Center dismiss button
        Box(
            modifier = Modifier
                .offset {
                    val offsetPx = with(density) { 21.dp.roundToPx() }
                    IntOffset(fabCenterOffset.x - offsetPx, fabCenterOffset.y - offsetPx)
                }
                .size(42.dp)
                .background(Color(0xFF29B6F6), RoundedCornerShape(13.dp))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Rounded.Close, contentDescription = "Close", tint = Color(0xFF051520), modifier = Modifier.size(20.dp))
        }
    }
}

@Composable
private fun OpTile(op: FabOp, modifier: Modifier, alpha: Float, onClick: () -> Unit) {
    val bg   = if (op.isAsync) asyncOpBg   else PanelStrong
    val tint = if (op.isAsync) asyncOpTint else syncOpTint

    Box(
        modifier = modifier
            .size(46.dp)
            .graphicsLayer { this.alpha = alpha }
            .background(bg, RoundedCornerShape(13.dp))
            .border(1.dp, if (op.isAsync) asyncOpTint.copy(alpha = 0.35f) else BorderStrong, RoundedCornerShape(13.dp))
            .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Icon(imageVector = op.icon, contentDescription = op.label, tint = tint, modifier = Modifier.size(17.dp))
            Text(op.label, fontSize = 7.sp, fontWeight = FontWeight.SemiBold, color = tint, letterSpacing = 0.3.sp)
        }
    }
}
