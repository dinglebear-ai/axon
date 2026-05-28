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

enum class DotState { Running, Done, Failed, Idle, Warn }

private fun dotColor(state: DotState) = when (state) {
    DotState.Running -> Color(0xFF29B6F6)
    DotState.Done    -> Color(0xFF7DD3C7)
    DotState.Failed  -> Color(0xFFC78490)
    DotState.Idle    -> Color(0xFFA7BCC9)
    DotState.Warn    -> Color(0xFFC6A36B)
}

@Composable
fun AuroraStatusDot(state: DotState, size: Dp = 7.dp, modifier: Modifier = Modifier) {
    val infiniteTransition = rememberInfiniteTransition(label = "dot")
    val pulseAlpha by infiniteTransition.animateFloat(
        initialValue = 1f,
        targetValue = 0.35f,
        animationSpec = infiniteRepeatable(
            animation = tween(900, easing = FastOutSlowInEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "pulse",
    )
    val alpha = if (state == DotState.Running) pulseAlpha else 1f
    Box(modifier = modifier.size(size).clip(CircleShape).background(dotColor(state).copy(alpha = alpha)))
}
