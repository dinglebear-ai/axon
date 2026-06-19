package com.axon.app.ui.status

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
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
    onOfflineClick: (() -> Unit)? = null,
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
        ConnectionState.Checking -> "Checking"
        ConnectionState.Online -> latencyMs?.let { "${it.coerceAtMost(999)}ms" } ?: "Online"
        ConnectionState.Offline -> "Offline"
    }
    val shape = RoundedCornerShape(999.dp)
    val onClick = if (state == ConnectionState.Offline && onOfflineClick != null) onOfflineClick else vm::refresh

    Row(
        modifier = modifier
            .height(30.dp)
            .background(colors.control.copy(alpha = 0.42f), shape)
            .border(1.dp, dot.copy(alpha = 0.34f), shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(
            modifier = Modifier
                .size(6.dp)
                .clip(shape)
                .background(dot.copy(alpha = 0.92f)),
        )
        Text(
            label,
            color = if (state == ConnectionState.Offline) colors.textPrimary.copy(alpha = 0.88f) else colors.textMuted.copy(alpha = 0.86f),
            fontSize = 11.2.sp,
            lineHeight = 14.sp,
            fontFamily = AxonTheme.fonts.body,
        )
    }
}
