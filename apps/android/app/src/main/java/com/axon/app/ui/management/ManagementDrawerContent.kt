package com.axon.app.ui.management

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.axon.app.ui.common.DrawerSubItem
import com.axon.app.ui.status.ConnectionState
import com.axon.app.ui.status.ConnectionStatusViewModel
import com.axon.app.ui.theme.AxonColors

// ViewModel is activity-scoped (default ViewModelStoreOwner), so stats results
// survive across drawer open/close cycles — intentional, avoids redundant network calls.
@Composable
fun ManagementDrawerContent(
    onOpenSettings: () -> Unit,
    statusVm: ConnectionStatusViewModel = viewModel(),
    vm: ManagementViewModel = viewModel(),
) {
    val connState by statusVm.state.collectAsStateWithLifecycle()
    val statsState by vm.statsState.collectAsStateWithLifecycle()

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        // ── Connection status header ──────────────────────────────────────────
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text("Server", style = MaterialTheme.typography.labelSmall, color = AxonColors.TextMuted)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                val (dotColor, label) = when (connState) {
                    ConnectionState.Checking -> AxonColors.AccentPrimary to "Checking"
                    ConnectionState.Online   -> AxonColors.SuccessBase to "Online"
                    ConnectionState.Offline  -> AxonColors.ErrorBase to "Offline"
                }
                Box(modifier = Modifier.size(7.dp)) {
                    Canvas(modifier = Modifier.fillMaxSize()) {
                        drawCircle(color = dotColor)
                    }
                }
                Text(label, style = MaterialTheme.typography.labelSmall, color = dotColor)
                Text("·", style = MaterialTheme.typography.labelSmall, color = AxonColors.TextMuted)
                Text(
                    "Refresh",
                    style = MaterialTheme.typography.labelSmall,
                    color = AxonColors.AccentPrimary,
                    modifier = Modifier
                        .minimumInteractiveComponentSize()
                        .clickable(remember { MutableInteractionSource() }, indication = null) {
                            statusVm.refresh()
                        },
                )
            }
        }

        // ── Monitor ───────────────────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.MonitorHeart,
            label = "Monitor",
            detail = when (connState) {
                ConnectionState.Checking -> "Checking…"
                ConnectionState.Online   -> "Server reachable"
                ConnectionState.Offline  -> "Server unreachable"
            },
            detailColor = when (connState) {
                ConnectionState.Checking -> AxonColors.TextMuted
                ConnectionState.Online   -> AxonColors.SuccessBase
                ConnectionState.Offline  -> AxonColors.ErrorBase
            },
        )

        // ── Stack (stats) ─────────────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.Storage,
            label = "Stack",
            detail = when (val s = statsState) {
                is MgmtActionState.Idle    -> "Tap to load"
                is MgmtActionState.Loading -> "Loading…"
                is MgmtActionState.Done    -> s.summary
                is MgmtActionState.Error   -> s.message
            },
            detailColor = if (statsState is MgmtActionState.Error) AxonColors.ErrorBase else AxonColors.TextMuted,
            onClick = { vm.loadStats() },
        )

        // ── Dedupe (coming soon) ──────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.ContentCopy,
            label = "Dedupe",
            detail = "Coming soon",
            detailColor = AxonColors.TextMuted,
            trailing = { Text("soon", style = MaterialTheme.typography.labelSmall, color = AxonColors.WarnBase) },
        )

        // ── Sync (coming soon) ────────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.Sync,
            label = "Sync",
            detail = "Coming soon",
            detailColor = AxonColors.TextMuted,
            trailing = { Text("soon", style = MaterialTheme.typography.labelSmall, color = AxonColors.WarnBase) },
        )

        // ── Config ────────────────────────────────────────────────────────────
        DrawerSubItem(
            icon = Icons.Rounded.Tune,
            label = "Config",
            detail = "Server URL, token, collection",
            detailColor = AxonColors.TextMuted,
            onClick = onOpenSettings,
        )
    }
}
