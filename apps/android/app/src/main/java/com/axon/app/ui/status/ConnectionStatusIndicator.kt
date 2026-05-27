package com.axon.app.ui.status

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import tv.tootie.aurora.components.AuroraStatusIndicator
import tv.tootie.aurora.components.AuroraStatusTone

/**
 * Compact connection-status pill for the top app bar. Tap to force-refresh.
 *
 * Mapping: `Checking → Syncing` (info, pulses), `Online → Online` (success),
 * `Offline → Offline` (neutral). All tones resolve to Aurora token colors.
 */
@Composable
fun ConnectionStatusIndicator(vm: ConnectionStatusViewModel = viewModel()) {
    val state by vm.state.collectAsStateWithLifecycle()
    val (tone, label) = when (state) {
        ConnectionState.Checking -> AuroraStatusTone.Syncing to "Checking"
        ConnectionState.Online   -> AuroraStatusTone.Online to "Online"
        ConnectionState.Offline  -> AuroraStatusTone.Offline to "Offline"
    }
    AuroraStatusIndicator(
        tone = tone,
        label = label,
        modifier = Modifier
            .clickable { vm.refresh() }
            .padding(horizontal = 8.dp, vertical = 4.dp),
    )
}
