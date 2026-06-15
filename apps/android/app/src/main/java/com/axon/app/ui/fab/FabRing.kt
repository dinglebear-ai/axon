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
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.axon.app.ui.common.pressScale
import com.axon.app.ui.theme.AxonTheme
import com.axon.app.ui.theme.AxonTone
import com.axon.app.ui.theme.tint
import com.axon.app.ui.theme.toneOf

private const val OP_COLUMNS = 3

@Composable
fun FabRing(
    visible: Boolean,
    @Suppress("UNUSED_PARAMETER") fabCenterOffset: IntOffset,
    onOpSelected: (FabOp) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val openProgress by animateFloatAsState(
        targetValue = if (visible) 1f else 0f,
        animationSpec = spring(dampingRatio = Spring.DampingRatioLowBouncy, stiffness = Spring.StiffnessMedium),
        label = "ops-open",
    )
    if (!visible && openProgress == 0f) return

    val colors = AxonTheme.colors
    Box(modifier = modifier.fillMaxSize()) {
        // Dim backdrop — tap to dismiss.
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(Color(0xFF040A0E).copy(alpha = openProgress * 0.9f))
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
        )

        // Bottom-anchored labeled grid: thumb-reachable, and every action carries
        // its name so the icon doesn't have to be self-explanatory.
        Column(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .widthIn(max = 440.dp)
                .padding(horizontal = 12.dp)
                .padding(bottom = 24.dp)
                .navigationBarsPadding()
                .graphicsLayer {
                    alpha = openProgress
                    translationY = (1f - openProgress) * 64f
                }
                .clip(RoundedCornerShape(20.dp))
                .background(colors.panelStrong.copy(alpha = 0.97f), RoundedCornerShape(20.dp))
                .border(1.dp, colors.tint(colors.accentPrimary, 16, colors.panelStrong), RoundedCornerShape(20.dp))
                .padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(9.dp),
        ) {
            Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    "Operations",
                    color = colors.textPrimary,
                    fontSize = 14.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = AxonTheme.fonts.display,
                    modifier = Modifier.weight(1f),
                )
                Box(
                    modifier = Modifier
                        .size(30.dp)
                        .clip(RoundedCornerShape(9.dp))
                        .pressScale(onClick = onDismiss),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        Icons.Rounded.Close,
                        contentDescription = "Close operations",
                        tint = colors.textMuted,
                        modifier = Modifier.size(17.dp),
                    )
                }
            }
            FabRingOps.chunked(OP_COLUMNS).forEach { rowOps ->
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    rowOps.forEach { op ->
                        OpTile(op = op, modifier = Modifier.weight(1f), onClick = { onOpSelected(op) })
                    }
                    repeat(OP_COLUMNS - rowOps.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
}

@Composable
private fun OpTile(op: FabOp, modifier: Modifier, onClick: () -> Unit) {
    val colors = AxonTheme.colors
    val tone = colors.toneOf(if (op.isAsync) AxonTone.Orange else AxonTone.Cyan)
    Column(
        modifier = modifier
            .clip(RoundedCornerShape(13.dp))
            .background(colors.tint(tone.base, 6, colors.pageBg), RoundedCornerShape(13.dp))
            .border(1.dp, colors.tint(tone.base, 18, colors.panelStrong), RoundedCornerShape(13.dp))
            .pressScale(onClick = onClick)
            .padding(vertical = 12.dp, horizontal = 5.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        Box(
            modifier = Modifier
                .size(38.dp)
                .clip(RoundedCornerShape(10.dp))
                .background(colors.tint(tone.base, 15, colors.pageBg), RoundedCornerShape(10.dp))
                .border(1.dp, colors.tint(tone.base, 32, colors.panelStrong), RoundedCornerShape(10.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(op.icon, contentDescription = null, tint = tone.fg, modifier = Modifier.size(19.dp))
        }
        Text(
            op.label,
            color = colors.textPrimary.copy(alpha = 0.92f),
            fontSize = 11.5.sp,
            fontFamily = AxonTheme.fonts.body,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center,
        )
    }
}
