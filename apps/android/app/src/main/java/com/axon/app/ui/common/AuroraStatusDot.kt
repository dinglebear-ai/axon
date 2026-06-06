package com.axon.app.ui.common

import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.axon.app.ui.theme.AxonTheme

enum class DotState { Running, Done, Failed, Idle, Warn }

private fun dotColor(state: DotState, colors: com.axon.app.ui.theme.AxonPalette) = when (state) {
    DotState.Running -> colors.accentPrimary
    DotState.Done    -> colors.success
    DotState.Failed  -> colors.error
    DotState.Idle    -> colors.textMuted
    DotState.Warn    -> colors.warn
}

@Composable
fun AuroraStatusDot(state: DotState, size: Dp = 7.dp, modifier: Modifier = Modifier) {
    val color = dotColor(state, AxonTheme.colors)
    val alpha = if (state == DotState.Running) {
        val infiniteTransition = rememberInfiniteTransition(label = "dot")
        val pulseAlpha by infiniteTransition.animateFloat(
            initialValue = 1f,
            targetValue = 0.35f,
            animationSpec = infiniteRepeatable(
                animation = tween(900, easing = LinearEasing),
                repeatMode = RepeatMode.Reverse,
            ),
            label = "pulse",
        )
        pulseAlpha
    } else {
        1f
    }
    Box(modifier = modifier.size(size).clip(CircleShape).background(color.copy(alpha = alpha)))
}
