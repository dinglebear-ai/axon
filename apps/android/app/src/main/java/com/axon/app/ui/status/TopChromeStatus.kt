package com.axon.app.ui.status

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.theme.AxonTheme

@Composable
fun TopChromeStatus(
    modifier: Modifier = Modifier,
    vm: ConnectionStatusViewModel = viewModel(),
) {
    val colors = AxonTheme.colors
    val state by vm.state.collectAsStateWithLifecycle()
    val latencyMs by vm.latencyMs.collectAsStateWithLifecycle()
    val dot = when (state) {
        ConnectionState.Checking -> colors.accentStrong
        ConnectionState.Online -> colors.success
        ConnectionState.Offline -> colors.error
    }
    val label = when (state) {
        ConnectionState.Checking -> "..."
        ConnectionState.Online -> latencyMs?.let { "${it.coerceAtMost(999)}ms" } ?: "live"
        ConnectionState.Offline -> "down"
    }

    Row(
        modifier = modifier.clickable { vm.refresh() },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Box(
            modifier = Modifier
                .size(5.5.dp)
                .clip(CircleShape)
                .background(dot.copy(alpha = 0.92f)),
        )
        Text(
            label,
            color = colors.textMuted.copy(alpha = 0.76f),
            fontSize = 9.4.sp,
            fontFamily = AxonTheme.fonts.mono,
        )
    }
}
