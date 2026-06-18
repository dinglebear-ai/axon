package com.axon.app.ui.common

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone

enum class DotState { Running, Done, Failed, Idle, Warn }

private fun dotTone(state: DotState): AuroraStatusTone = when (state) {
    DotState.Running -> AuroraStatusTone.Syncing
    DotState.Done    -> AuroraStatusTone.Online
    DotState.Failed  -> AuroraStatusTone.Error
    DotState.Idle    -> AuroraStatusTone.Offline
    DotState.Warn    -> AuroraStatusTone.Degraded
}

@Composable
fun AuroraStatusDot(state: DotState, size: Dp = 7.dp, modifier: Modifier = Modifier) {
    AuroraStatusIndicator(
        tone = dotTone(state),
        modifier = modifier,
        label = null,
        dotSize = size,
        pulse = state == DotState.Running,
    )
}
